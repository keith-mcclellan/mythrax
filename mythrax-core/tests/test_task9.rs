use mythrax_core::cognitive::synthesis::{CONCISION_DIRECTIVE, build_synthesis_prompt, check_compression_ratio};
use tracing_subscriber::{EnvFilter, fmt::Subscriber};
use std::sync::{Arc, Mutex};
use tracing::Subscriber as TracingSubscriber;

#[test]
fn test_concision_prompt_prepending() {
    let base_prompt = "You are a systems synthesizer.";
    let final_prompt = build_synthesis_prompt(base_prompt);
    
    assert!(final_prompt.starts_with(CONCISION_DIRECTIVE));
    assert!(final_prompt.contains(base_prompt));
}

#[derive(Default, Clone)]
struct MockWarningSubscriber {
    warnings: Arc<Mutex<Vec<String>>>,
}

impl tracing::Subscriber for MockWarningSubscriber {
    fn enabled(&self, _metadata: &tracing::Metadata<'_>) -> bool {
        true
    }
    fn new_span(&self, _span: &tracing::span::Attributes<'_>) -> tracing::span::Id {
        tracing::span::Id::from_u64(1)
    }
    fn record(&self, _span: &tracing::span::Id, _values: &tracing::span::Record<'_>) {}
    fn record_follows_from(&self, _span: &tracing::span::Id, _follows: &tracing::span::Id) {}
    fn event(&self, event: &tracing::Event<'_>) {
        if event.metadata().level() == &tracing::Level::WARN {
            let mut visitor = StringVisitor::default();
            event.record(&mut visitor);
            self.warnings.lock().unwrap().push(visitor.0);
        }
    }
    fn enter(&self, _span: &tracing::span::Id) {}
    fn exit(&self, _span: &tracing::span::Id) {}
}

#[derive(Default)]
struct StringVisitor(String);

impl tracing::field::Visit for StringVisitor {
    fn record_debug(&mut self, field: &tracing::field::Field, value: &dyn std::fmt::Debug) {
        if field.name() == "message" {
            self.0 = format!("{:?}", value);
        }
    }
}

#[test]
fn test_compression_warning_triggers() {
    let subscriber = MockWarningSubscriber::default();
    let warnings = subscriber.warnings.clone();
    
    // Set the env var for the test
    unsafe {
        std::env::set_var("MYTHRAX_VERBOSITY_ALERT_RATIO", "1.5");
    }
    
    // Run inside subscriber dispatcher
    tracing::subscriber::with_default(subscriber, || {
        // Mock large input and output compared to original tokens
        // input_tokens + output_tokens / original_tokens > 1.5
        // Let input_text length be 800 (200 tokens)
        // Let output_text length be 400 (100 tokens)
        // total tokens = 300
        // original_tokens = 100
        // ratio = 3.0 > 1.5 -> Should trigger warning
        let input_text = "a".repeat(800);
        let output_text = "a".repeat(400);
        
        check_compression_ratio(&input_text, &output_text, 100);
        
        // original_tokens = 300
        // ratio = 1.0 < 1.5 -> Should not trigger warning
        check_compression_ratio(&input_text, &output_text, 300);
    });
    
    let w = warnings.lock().unwrap();
    assert_eq!(w.len(), 1);
    assert!(w[0].contains("Verbosity alert: compression ratio"));
}
