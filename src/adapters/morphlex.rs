use super::adapter::*;
use async_trait::async_trait;
use libc::{c_char, c_int, size_t};
use std::ffi::{CStr, CString};
use std::mem;
use std::ptr;
use std::time::Instant;

/// Morphlex adapter for in-house Morphlex LLM via ONNX inference
pub struct MorphlexAdapter {
    model_name: String,
    /// Path to the ONNX model file
    model_path: String,
    /// Path to libmorphlex.so for FFI
    ffi_lib_path: String,
    /// Loaded library handle
    lib_handle: Option<libloading::Library>,
    /// Function pointer for morphlex_compile
    morphlex_compile: Option<unsafe extern "C" fn(
        *const c_char, 
        size_t, 
        *mut TokenVector, 
        *mut size_t, 
        *mut *mut c_char, 
        *mut size_t
    ) -> c_int>,
    timeout_ms: u64,
    /// Cost per token (in-house, typically lower than commercial APIs)
    cost_per_1k_tokens: f64,
}

#[repr(C, packed)]
#[derive(Debug, Clone, Copy)]
pub struct TokenVector {
    pub id: i32,
    pub lemma_id: i32,
    pub pos: i8,
    pub role: i8,
    pub morph: i16,
}

impl MorphlexAdapter {
    pub fn new(model_path: String, ffi_lib_path: String) -> Self {
        let mut adapter = MorphlexAdapter {
            model_name: "morphlex-local".to_string(),
            model_path,
            ffi_lib_path,
            lib_handle: None,
            morphlex_compile: None,
            timeout_ms: 30000,
            cost_per_1k_tokens: 0.01, // In-house cost
        };
        
        // Try to load the library and get function pointers
        let _ = adapter.load_library();
        
        adapter
    }

    pub fn with_config(
        model_path: String,
        ffi_lib_path: String,
        cost_per_1k_tokens: f64,
        timeout_ms: u64,
    ) -> Self {
        let mut adapter = MorphlexAdapter {
            model_name: "morphlex-local".to_string(),
            model_path,
            ffi_lib_path,
            lib_handle: None,
            morphlex_compile: None,
            timeout_ms,
            cost_per_1k_tokens,
        };
        
        // Try to load the library and get function pointers
        let _ = adapter.load_library();
        
        adapter
    }

    fn load_library(&mut self) -> Result<(), Box<dyn std::error::Error>> {
        // Safety: We're loading a library we trust (our own compiled libmorphlex.so)
        unsafe {
            let lib = libloading::Library::new(&self.ffi_lib_path)?;
            self.lib_handle = Some(lib);
            
            // Get the morphlex_compile function
            let compile_func: libloading::Symbol<unsafe extern "C" fn(
                *const c_char, 
                size_t, 
                *mut TokenVector, 
                *mut size_t, 
                *mut *mut c_char, 
                *mut size_t
            ) -> c_int> = self.lib_handle.as_ref().unwrap().get(b"morphlex_compile\0")?;
            
            self.morphlex_compile = Some(*compile_func);
        }
        
        Ok(())
    }

    /// Call the Morphlex inference via FFI
    fn call_morphlex(&self, prompt: &str, _max_tokens: u32) -> Result<String, AgentError> {
        // Ensure library is loaded
        if self.lib_handle.is_none() || self.morphlex_compile.is_none() {
            return Err(AgentError::Api(
                "Morphlex library not loaded".to_string()
            ));
        }

        // Safety: We're calling into our own FFI function with valid parameters
        unsafe {
            // Convert prompt to CString
            let c_prompt = CString::new(prompt.as_bytes())?;
            
            // Prepare output buffers (we'll start with a reasonable size)
            let mut token_count: size_t = 0;
            let mut lemma_count: size_t = 0;
            let mut tokens_ptr: *mut TokenVector = ptr::null_mut();
            let mut lemmas_ptr: *mut *mut c_char = ptr::null_mut();
            
            // Call the morphlex_compile function
            let result = (self.morphlex_compile.unwrap())(
                c_prompt.as_ptr(),
                prompt.len() as size_t,
                tokens_ptr,
                &mut token_count,
                lemmas_ptr,
                &mut lemma_count,
            );
            
            if result != 0 {
                return Err(AgentError::Api(
                    format!("Morphlex inference failed with error code: {}", result)
                ));
            }
            
            // If we got here with null pointers, we need to allocate buffers and call again
            if tokens_ptr.is_null() && token_count > 0 {
                // Allocate buffer for tokens
                 tokens_ptr = libc::malloc((token_count * mem::size_of::<TokenVector>()) as size_t) as *mut TokenVector
                    as *mut TokenVector;
                
                if tokens_ptr.is_null() {
                    return Err(AgentError::Api(
                        "Failed to allocate memory for tokens".to_string()
                    ));
                }
                
                // Allocate buffer for lemma pointers
                 lemmas_ptr = libc::malloc((lemma_count * mem::size_of::<*mut c_char>()) as size_t) as *mut *mut c_char
                    as *mut *mut c_char;
                
                if lemmas_ptr.is_null() && lemma_count > 0 {
                    libc::free(tokens_ptr as *mut libc::c_void);
                    return Err(AgentError::Api(
                        "Failed to allocate memory for lemmas".to_string()
                    ));
                }
                
                // Call again with allocated buffers
                let result = (self.morphlex_compile.unwrap())(
                    c_prompt.as_ptr(),
                    prompt.len() as size_t,
                    tokens_ptr,
                    &mut token_count,
                    lemmas_ptr,
                    &mut lemma_count,
                );
                
                if result != 0 {
                    // Clean up on failure
                    if !tokens_ptr.is_null() {
                        libc::free(tokens_ptr as *mut libc::c_void);
                    }
                    if !lemmas_ptr.is_null() {
                        libc::free(lemmas_ptr as *mut libc::c_void);
                    }
                    return Err(AgentError::Api(
                        format!("Morphlex inference failed with error code: {}", result)
                    ));
                }
            }
            
            // Process results
            let mut result_text = String::new();
            
            // Add tokens information
            if !tokens_ptr.is_null() && token_count > 0 {
                let tokens = std::slice::from_raw_parts(tokens_ptr, token_count as usize);
                result_text.push_str(&format!("Tokens ({}): ", token_count));
                for (i, token) in tokens.iter().enumerate() {
                    if i > 0 {
                        result_text.push_str(", ");
                    }
                    let id = token.id;
                    let lemma_id = token.lemma_id;
                    let pos = token.pos;
                    let role = token.role;
                    let morph = token.morph;
                    result_text.push_str(&format!(
                        "{{id: {}, lemma_id: {}, pos: {}, role: {}, morph: {}}}", 
                        id, lemma_id, pos, role, morph
                    ));
                }
                result_text.push_str("\n");
                
                // Free tokens buffer
                libc::free(tokens_ptr as *mut libc::c_void);
            }
            
            // Add lemmas information
            if !lemmas_ptr.is_null() && lemma_count > 0 {
                let lemmas = std::slice::from_raw_parts(lemmas_ptr, lemma_count as usize);
                result_text.push_str(&format!("Lemmas ({}): ", lemma_count));
                for (i, lemma_ptr) in lemmas.iter().enumerate() {
                    if i > 0 {
                        result_text.push_str(", ");
                    }
                    if !lemma_ptr.is_null() {
                        let lemma_cstr = CStr::from_ptr(*lemma_ptr);
                        if let Ok(lemma_str) = lemma_cstr.to_str() {
                            result_text.push_str(lemma_str);
                        } else {
                            result_text.push_str("<invalid UTF-8>");
                        }
                    } else {
                        result_text.push_str("<null>");
                    }
                }
                result_text.push_str("\n");
                
                // Free lemmas (each string and the pointer array)
                for lemma_ptr in lemmas.iter() {
                    if !lemma_ptr.is_null() {
                        libc::free(*lemma_ptr as *mut libc::c_void);
                    }
                }
                libc::free(lemmas_ptr as *mut libc::c_void);
            }
            
            if result_text.is_empty() {
                result_text = "Morphlex inference completed but produced no output".to_string();
            }
            
            Ok(result_text)
        }
    }
}

#[async_trait]
impl AgentAdapter for MorphlexAdapter {
    fn backend_name(&self) -> &str {
        "morphlex"
    }

    fn model_name(&self) -> &str {
        &self.model_name
    }

     fn box_clone(&self) -> Box<dyn AgentAdapter> {
         Box::new(MorphlexAdapter {
             model_name: self.model_name.clone(),
             model_path: self.model_path.clone(),
             ffi_lib_path: self.ffi_lib_path.clone(),
             lib_handle: None,
             morphlex_compile: None,
             timeout_ms: self.timeout_ms,
             cost_per_1k_tokens: self.cost_per_1k_tokens,
         })
     }

    async fn request(&self, req: &AgentRequest) -> Result<AgentOutput, AgentError> {
        let start = Instant::now();

        // Build full prompt with system context
        let full_prompt = format!(
            "<|system|>{}\n\n<|user|>{}\n\n<|assistant|>",
            req.system, req.prompt
        );

        let content = self.call_morphlex(&full_prompt, req.max_tokens)?;
        let elapsed = start.elapsed().as_millis() as u64;

        // Estimate tokens by character count (rough heuristic)
        let prompt_tokens = (full_prompt.len() as f64 / 4.0).ceil() as u32;
        let completion_tokens = (content.len() as f64 / 4.0).ceil() as u32;

        Ok(AgentOutput {
            content: content.clone(),
            structured: if matches!(req.output_format, OutputFormat::Json) {
                serde_json::from_str(&content).ok()
            } else {
                None
            },
            usage: UsageStats {
                prompt_tokens,
                completion_tokens,
                total_tokens: prompt_tokens + completion_tokens,
                cost_cents: self.estimate_cost(prompt_tokens, completion_tokens),
                latency_ms: elapsed,
            },
        })
    }

    async fn health_check(&self) -> bool {
        std::path::Path::new(&self.model_path).exists()
            && std::path::Path::new(&self.ffi_lib_path).exists()
    }

    fn estimate_cost(&self, _prompt_tokens: u32, completion_tokens: u32) -> f64 {
        (completion_tokens as f64 / 1000.0) * self.cost_per_1k_tokens
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_morphlex_adapter_creation() {
        let adapter = MorphlexAdapter::new(
            "/path/to/model.onnx".to_string(),
            "/path/to/libmorphlex.so".to_string(),
        );
        assert_eq!(adapter.backend_name(), "morphlex");
        assert_eq!(adapter.model_name(), "morphlex-local");
    }

    #[test]
    fn test_morphlex_cost_estimate() {
        let adapter = MorphlexAdapter::new(
            "/path/to/model.onnx".to_string(),
            "/path/to/libmorphlex.so".to_string(),
        );
        // 1000 tokens at $0.01/1k = $0.01
        let cost = adapter.estimate_cost(500, 1000);
        assert!((cost - 0.01).abs() < 0.001);
    }
}
