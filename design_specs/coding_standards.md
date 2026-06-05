# Arena System Coding Standards

## Rust Specific Guidelines

### Error Handling
- Use `thiserror` for custom error types
- Propagate errors using `?` operator
- Provide meaningful error messages
- Use appropriate error variants for different failure modes

### Logging
- Use `tracing` crate for structured logging
- Log at appropriate levels (info, warn, error, debug)
- Include relevant context in log messages
- Use `info!` for general operational messages
- Use `error!` for failure conditions

### Code Organization
- Group related functionality in modules
- Keep functions focused and under 50 lines when possible
- Use descriptive names for variables and functions
- Follow Rust naming conventions (snake_case for functions/variables, PascalCase for types)

### Testing
- Write unit tests for all public functions
- Use table-driven tests when appropriate
- Test both success and failure cases
- Aim for high test coverage (>80%)

### Dependencies
- Keep dependencies minimal and well-justified
- Update dependencies regularly
- Use feature flags when appropriate
- Avoid unnecessary large dependencies

### Performance
- Avoid unnecessary allocations in hot paths
- Use appropriate data structures for the use case
- Consider lifetime implications in function signatures
- Profile performance-critical code

## General Principles

### Clarity over Cleverness
- Write code that is easy to understand
- Prefer explicit over implicit behavior
- Document non-obvious decisions

### Consistency
- Follow existing code patterns in the codebase
- Maintain consistent formatting
- Use the same approaches for similar problems

### Maintainability
- Write code that is easy to modify
- Avoid premature optimization
- Prefer composition over inheritance when applicable