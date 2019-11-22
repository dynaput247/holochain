use crate::{cloudwatch::CloudWatchLogger, logger::LoggerMetricPublisher, MetricPublisher};
use holochain_locksmith::RwLock;
use std::sync::Arc;

/// Unifies all possible metric publisher configurations
#[derive(Deserialize, Serialize, PartialEq, Debug, Clone)]
#[serde(rename_all = "lowercase")]
#[serde(tag = "type")]
pub enum MetricPublisherConfig {
    Logger,
    CloudWatchLogs {
        region: Option<rusoto_core::region::Region>,
        log_group_name: Option<String>,
        log_stream_name: Option<String>,
        assume_role_arn: Option<String>
    },
}

impl Default for MetricPublisherConfig {
    fn default() -> Self {
        Self::default_logger()
    }
}

impl MetricPublisherConfig {
    /// Instantiates a new metric publisher given this configuration instance.
    pub fn create_metric_publisher(&self) -> Arc<RwLock<dyn MetricPublisher>> {
        let publisher: Arc<RwLock<dyn MetricPublisher>> = match self {
            Self::Logger => Arc::new(RwLock::new(LoggerMetricPublisher::new())),
            Self::CloudWatchLogs {
                region,
                log_group_name,
                log_stream_name,
                assume_role_arn
            } => {
                let region = region.clone().unwrap_or_default();
                let assume_role_arn = assume_role_arn.clone()
                    .unwrap_or_else(crate::cloudwatch::CloudWatchLogger::default_assume_role_arn);
                let provider = crate::cloudwatch::assume_role(&region, &assume_role_arn);
                Arc::new(RwLock::new(CloudWatchLogger::new(
                    log_stream_name.clone(),
                    log_group_name.clone(),
                    provider,
                    &region,
                )))
            }
        };
        publisher
    }

    /// The default logger metric publisher configuration.
    pub fn default_logger() -> Self {
        Self::Logger
    }

    /// The default cloudwatch logger publisher configuration.
    pub fn default_cloudwatch_logs() -> Self {
        Self::CloudWatchLogs {
            region: Default::default(),
            log_group_name: Some(crate::cloudwatch::CloudWatchLogger::default_log_group()),
            log_stream_name: Some(crate::cloudwatch::CloudWatchLogger::default_log_stream()),
            assume_role_arn: Some(crate::cloudwatch::CloudWatchLogger::default_assume_role_arn())
        }
    }
}
