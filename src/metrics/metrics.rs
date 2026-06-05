use prometheus::{
    CounterVec, HistogramVec, IntCounter, IntGauge, Registry, Opts,
    HistogramOpts,
};
use std::sync::OnceLock;

static METRICS: OnceLock<ArenaMetrics> = OnceLock::new();

/// Arena metrics collector
pub struct ArenaMetrics {
    registry: Registry,
    /// Total sessions created
    sessions_total: IntCounter,
    /// Active sessions currently in progress
    active_sessions: IntGauge,
    /// Sessions completed (finalized)
    sessions_completed: IntCounter,
    /// Sessions cancelled
    sessions_cancelled: IntCounter,
    /// Agent responses by agent name
    agent_responses: CounterVec,
    /// Agent response latency (seconds)
    agent_response_latency: HistogramVec,
    /// Agent response cost (cents)
    agent_response_cost: CounterVec,
    /// Consistency scores
    consistency_score: HistogramVec,
    /// Council evaluations by recommendation
    council_evaluations: CounterVec,
    /// Drift findings by severity
    drift_findings: CounterVec,
    /// Human decisions by type
    human_decisions: CounterVec,
}

impl ArenaMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let sessions_total = IntCounter::new(
            "arena_sessions_total",
            "Total arena sessions created"
        )?;

        let active_sessions = IntGauge::new(
            "arena_active_sessions",
            "Currently active arena sessions"
        )?;

        let sessions_completed = IntCounter::new(
            "arena_sessions_completed",
            "Total arena sessions finalized"
        )?;

        let sessions_cancelled = IntCounter::new(
            "arena_sessions_cancelled",
            "Total arena sessions cancelled"
        )?;

        let agent_responses = CounterVec::new(
            Opts::new(
                "arena_agent_responses_total",
                "Total agent responses by agent name"
            ),
            &["agent_name", "backend"]
        )?;

        let agent_response_latency = HistogramVec::new(
            HistogramOpts::new(
                "arena_agent_response_latency_seconds",
                "Agent response latency in seconds"
            ),
            &["agent_name", "backend"]
        )?;

        let agent_response_cost = CounterVec::new(
            Opts::new(
                "arena_agent_response_cost_cents_total",
                "Agent response cost in cents"
            ),
            &["agent_name", "backend"]
        )?;

        let consistency_score = HistogramVec::new(
            HistogramOpts::new(
                "arena_consistency_score",
                "Consistency score across agent responses (0-1)"
            ).buckets(vec![0.0, 0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9, 1.0]),
            &["session_type"]
        )?;

        let council_evaluations = CounterVec::new(
            Opts::new(
                "arena_council_evaluations_total",
                "Council evaluations by recommendation type"
            ),
            &["recommendation"]
        )?;

        let drift_findings = CounterVec::new(
            Opts::new(
                "arena_drift_findings_total",
                "Spec drift findings by severity"
            ),
            &["severity"]
        )?;

        let human_decisions = CounterVec::new(
            Opts::new(
                "arena_human_decisions_total",
                "Human decisions by type"
            ),
            &["decision"]
        )?;

        registry.register(Box::new(sessions_total.clone()))?;
        registry.register(Box::new(active_sessions.clone()))?;
        registry.register(Box::new(sessions_completed.clone()))?;
        registry.register(Box::new(sessions_cancelled.clone()))?;
        registry.register(Box::new(agent_responses.clone()))?;
        registry.register(Box::new(agent_response_latency.clone()))?;
        registry.register(Box::new(agent_response_cost.clone()))?;
        registry.register(Box::new(consistency_score.clone()))?;
        registry.register(Box::new(council_evaluations.clone()))?;
        registry.register(Box::new(drift_findings.clone()))?;
        registry.register(Box::new(human_decisions.clone()))?;

        Ok(ArenaMetrics {
            registry,
            sessions_total,
            active_sessions,
            sessions_completed,
            sessions_cancelled,
            agent_responses,
            agent_response_latency,
            agent_response_cost,
            consistency_score,
            council_evaluations,
            drift_findings,
            human_decisions,
        })
    }

    /// Initialize global metrics instance
    pub fn init() -> &'static Self {
        METRICS.get_or_init(|| {
            ArenaMetrics::new().expect("Failed to initialize arena metrics")
        })
    }

    /// Get the global metrics instance
    pub fn global() -> &'static Self {
        METRICS.get().expect("Arena metrics not initialized")
    }

    /// Get the Prometheus registry
    pub fn registry(&self) -> &Registry {
        &self.registry
    }

    // Recording methods

    pub fn record_session_created(&self) {
        self.sessions_total.inc();
        self.active_sessions.inc();
    }

    pub fn record_session_finalized(&self) {
        self.sessions_completed.inc();
        self.active_sessions.dec();
    }

    pub fn record_session_cancelled(&self) {
        self.sessions_cancelled.inc();
        self.active_sessions.dec();
    }

    pub fn record_agent_response(&self, agent_name: &str, backend: &str, latency_ms: u64, cost_cents: f64) {
        self.agent_responses
            .with_label_values(&[agent_name, backend])
            .inc();
        self.agent_response_latency
            .with_label_values(&[agent_name, backend])
            .observe(latency_ms as f64 / 1000.0);
        self.agent_response_cost
            .with_label_values(&[agent_name, backend])
            .inc_by(cost_cents);
    }

    pub fn record_consistency_score(&self, session_type: &str, score: f64) {
        self.consistency_score
            .with_label_values(&[session_type])
            .observe(score);
    }

    pub fn record_council_evaluation(&self, recommendation: &str) {
        self.council_evaluations
            .with_label_values(&[recommendation])
            .inc();
    }

    pub fn record_drift_finding(&self, severity: &str) {
        self.drift_findings
            .with_label_values(&[severity])
            .inc();
    }

    pub fn record_human_decision(&self, decision: &str) {
        self.human_decisions
            .with_label_values(&[decision])
            .inc();
    }

    /// Export metrics in Prometheus format
    pub fn export(&self) -> String {
        use prometheus::Encoder;
        let encoder = prometheus::TextEncoder::new();
        let mut buffer = Vec::new();
        encoder.encode(&self.registry.gather(), &mut buffer).unwrap();
        String::from_utf8(buffer).unwrap()
    }
}
