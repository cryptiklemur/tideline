use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::{Duration, Instant};
use serde_json::Value;
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};

const RATE_WINDOW: Duration = Duration::from_secs(1);
const RATE_MAX_EVENTS: usize = 200;
const CHANNEL_DEPTH: usize = 1024;

#[derive(Debug, Error)]
pub enum EventError {
    #[error("plugin {0} is not registered with the bus")]
    PluginUnknown(String),
    #[error("topic {topic:?} is not declared by plugin {plugin_id:?}")]
    TopicUndeclared { plugin_id: String, topic: String },
}

#[derive(Debug, Clone)]
pub struct Event {
    pub topic: String,
    pub params: Value,
}

struct PluginInbox {
    sender: mpsc::Sender<Event>,
    subscriptions: HashSet<String>,
    declared_topics: HashSet<String>,
    rate_window_start: Instant,
    rate_count: usize,
    drops: u64,
}

#[derive(Default)]
pub struct EventBusInner {
    inboxes: HashMap<String, PluginInbox>,
}

#[derive(Clone, Default)]
pub struct EventBus {
    inner: Arc<Mutex<EventBusInner>>,
}

impl EventBus {
    pub fn new() -> Self { Self::default() }

    pub async fn register_plugin(
        &self,
        plugin_id: &str,
        declared_topics: Vec<String>,
    ) -> mpsc::Receiver<Event> {
        let (tx, rx) = mpsc::channel(CHANNEL_DEPTH);
        let mut inner = self.inner.lock().await;
        inner.inboxes.insert(plugin_id.to_string(), PluginInbox {
            sender: tx,
            subscriptions: HashSet::new(),
            declared_topics: declared_topics.into_iter().collect(),
            rate_window_start: Instant::now(),
            rate_count: 0,
            drops: 0,
        });
        rx
    }

    pub async fn unregister_plugin(&self, plugin_id: &str) {
        let mut inner = self.inner.lock().await;
        inner.inboxes.remove(plugin_id);
    }

    pub async fn subscribe(&self, plugin_id: &str, topic: &str) -> Result<(), EventError> {
        let mut inner = self.inner.lock().await;
        let inbox = inner.inboxes.get_mut(plugin_id)
            .ok_or_else(|| EventError::PluginUnknown(plugin_id.into()))?;
        inbox.subscriptions.insert(topic.into());
        Ok(())
    }

    pub async fn unsubscribe(&self, plugin_id: &str, topic: &str) -> Result<(), EventError> {
        let mut inner = self.inner.lock().await;
        let inbox = inner.inboxes.get_mut(plugin_id)
            .ok_or_else(|| EventError::PluginUnknown(plugin_id.into()))?;
        inbox.subscriptions.remove(topic);
        Ok(())
    }

    pub async fn publish_host(&self, topic: &str, params: Value) {
        let event = Event { topic: topic.into(), params };
        self.fanout(event).await;
    }

    pub async fn publish_plugin(
        &self,
        plugin_id: &str,
        topic: &str,
        params: Value,
    ) -> Result<(), EventError> {
        {
            let inner = self.inner.lock().await;
            let inbox = inner.inboxes.get(plugin_id)
                .ok_or_else(|| EventError::PluginUnknown(plugin_id.into()))?;
            if !inbox.declared_topics.contains(topic) {
                return Err(EventError::TopicUndeclared {
                    plugin_id: plugin_id.into(),
                    topic: topic.into(),
                });
            }
        }
        self.fanout(Event { topic: topic.into(), params }).await;
        Ok(())
    }

    pub async fn drops_for(&self, plugin_id: &str) -> u64 {
        let inner = self.inner.lock().await;
        inner.inboxes.get(plugin_id).map(|i| i.drops).unwrap_or(0)
    }

    async fn fanout(&self, event: Event) {
        let mut inner = self.inner.lock().await;
        let now = Instant::now();
        let topic = event.topic.clone();
        for (_id, inbox) in inner.inboxes.iter_mut() {
            if !inbox.subscriptions.contains(&topic) {
                continue;
            }
            if now.duration_since(inbox.rate_window_start) >= RATE_WINDOW {
                inbox.rate_window_start = now;
                inbox.rate_count = 0;
            }
            if inbox.rate_count >= RATE_MAX_EVENTS {
                inbox.drops += 1;
                continue;
            }
            inbox.rate_count += 1;
            if inbox.sender.try_send(event.clone()).is_err() {
                inbox.drops += 1;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[tokio::test]
    async fn fanout_only_to_subscribers() {
        let bus = EventBus::new();
        let mut a = bus.register_plugin("a", vec![]).await;
        let mut b = bus.register_plugin("b", vec![]).await;
        bus.subscribe("a", "topic.x").await.unwrap();
        bus.publish_host("topic.x", json!({"v":1})).await;
        let evt = a.recv().await.unwrap();
        assert_eq!(evt.topic, "topic.x");
        assert!(b.try_recv().is_err());
    }

    #[tokio::test]
    async fn publish_plugin_rejects_undeclared_topic() {
        let bus = EventBus::new();
        let _rx = bus.register_plugin("a", vec!["a:done".into()]).await;
        let err = bus.publish_plugin("a", "a:other", json!({})).await.unwrap_err();
        assert!(matches!(err, EventError::TopicUndeclared { .. }));
        bus.publish_plugin("a", "a:done", json!({})).await.unwrap();
    }

    #[tokio::test]
    async fn rate_limit_increments_drops() {
        let bus = EventBus::new();
        let mut rx = bus.register_plugin("a", vec![]).await;
        bus.subscribe("a", "spam").await.unwrap();
        for _ in 0..(RATE_MAX_EVENTS + 25) {
            bus.publish_host("spam", json!({})).await;
        }
        let mut received = 0;
        while rx.try_recv().is_ok() { received += 1; }
        assert!(received <= RATE_MAX_EVENTS, "got {received}");
        assert!(bus.drops_for("a").await >= 25);
    }
}
