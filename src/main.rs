use axum::{
    http::StatusCode,
    routing::{get, put},
    Json, Router,
};
use clap::{Args, Parser, Subcommand};
use log::{error, info, warn};
use rdkafka::client::ClientContext;
use rdkafka::config::{ClientConfig, RDKafkaLogLevel};
use rdkafka::consumer::stream_consumer::StreamConsumer;
use rdkafka::consumer::{CommitMode, Consumer, ConsumerContext, Rebalance};
use rdkafka::error::KafkaResult;
use rdkafka::producer::{FutureProducer, FutureRecord};
use rdkafka::topic_partition_list::TopicPartitionList;
use rdkafka::Message;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::{str, time::Duration};
use structured_logger::{async_json::new_writer, get_env_level, Builder};

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    Sender(Sender),
    Receiver(Receiver),
}

struct CustomContext;

impl ClientContext for CustomContext {}

impl ConsumerContext for CustomContext {
    fn pre_rebalance(&self, rebalance: &Rebalance) {
        info!("Pre rebalance {:?}", rebalance);
    }

    fn post_rebalance(&self, rebalance: &Rebalance) {
        info!("Post rebalance {:?}", rebalance);
    }

    fn commit_callback(&self, result: KafkaResult<()>, _offsets: &TopicPartitionList) {
        info!(
            "Committing offsets, result: {:?}, offsets: {:?}",
            result, _offsets
        );
    }
}

#[derive(Args, Debug)]
pub struct Receiver {
    #[arg(long)]
    kafka_address: String,

    #[arg(short, default_value_t = 8080)]
    port: u16,
}

const RECEIVER_KAFKA_RECEIVE_API: &str = "/kafka-receive";

#[derive(Serialize, Deserialize, Debug)]
struct KafkaReceiveBody {
    topic: String,
    key: Option<String>,
    payload: String,
    timestamp: Option<i64>,
}

impl Receiver {
    async fn receive_and_produce(&self) {
        let producer: FutureProducer = ClientConfig::new()
            .set("bootstrap.servers", &self.kafka_address)
            .set("message.timeout.ms", "5000")
            .set("client.id", "rdkafka-rs-receiver")
            .set_log_level(RDKafkaLogLevel::Error)
            .create()
            .expect("Producer creation failed");

        let app = Router::new()
            .route("/health", get(|| async { "OK" }))
            .route(
                RECEIVER_KAFKA_RECEIVE_API,
                put(|Json(body): Json<KafkaReceiveBody>| async move {
                    info!("Received message: {:?}", body);
                    let mut record = FutureRecord::to(&body.topic).payload(&body.payload);
                    if let Some(key) = &body.key {
                        record = record.key(key);
                    }
                    if let Some(timestamp) = body.timestamp {
                        record = record.timestamp(timestamp);
                    }
                    match producer.send(record, Duration::from_secs(0)).await {
                        Ok((partition, offset)) => {
                            info!(
                                "Sent message to kafka, partition: {}, offset: {}",
                                partition, offset
                            );
                            return (StatusCode::OK, Json(()));
                        }
                        Err((e, msg)) => {
                            error!(
                                "Error while sending message to kafka, error: {:?}, message: {:?}",
                                e, msg
                            );
                            return (StatusCode::BAD_REQUEST, Json(()));
                        }
                    };
                }),
            );

        let listener = tokio::net::TcpListener::bind(format!("0.0.0.0:{}", self.port))
            .await
            .unwrap();
        info!("Listening on port {}", self.port);
        axum::serve(listener, app).await.unwrap();
    }
}

#[derive(Args, Debug)]
pub struct Sender {
    #[arg(long)]
    kafka_address: String,

    #[arg(long)]
    kafka_topic: String,

    #[arg(long, default_value = "sender-group")]
    kafka_group_id: String,

    #[arg(long)]
    receiver_endpoint: String,
}

type LoggingConsumer = StreamConsumer<CustomContext>;

impl Sender {
    pub async fn consume_and_send(&self) {
        info!(
            "Running sender with args: {:?}, {:?}, {:?}, {:?}",
            self.kafka_address, self.kafka_topic, self.kafka_group_id, self.receiver_endpoint
        );
        let context = CustomContext;

        let consumer: LoggingConsumer = ClientConfig::new()
            .set("group.id", &self.kafka_group_id)
            .set("bootstrap.servers", &self.kafka_address)
            .set("enable.partition.eof", "false")
            .set("session.timeout.ms", "6000")
            .set("enable.auto.commit", "true")
            //.set("statistics.interval.ms", "30000")
            //.set("auto.offset.reset", "smallest")
            .set_log_level(RDKafkaLogLevel::Error)
            .create_with_context(context)
            .expect("Consumer creation failed");

        consumer
            .subscribe(&[&self.kafka_topic])
            .expect("Can't subscribe to specified topics");

        loop {
            match consumer.recv().await {
                Err(e) => {
                    error!("Kafka error: {}", e);
                }
                Ok(m) => {
                    let payload = match m.payload() {
                        None => "",
                        Some(s) => match str::from_utf8(s) {
                            Ok(s) => s,
                            Err(e) => {
                                warn!("Error while deserializing message payload: {:?}", e);
                                ""
                            }
                        },
                    };
                    info!("key: '{:?}', payload: '{}', topic: {}, partition: {}, offset: {}, timestamp: {:?}",
                          m.key(), payload, m.topic(), m.partition(), m.offset(), m.timestamp());
                    // send payload to receiver
                    let mut url = self.receiver_endpoint.clone();
                    url.push_str(RECEIVER_KAFKA_RECEIVE_API);
                    match reqwest::ClientBuilder::new()
                        .build()
                        .unwrap()
                        .put(url)
                        .body(
                            json!(KafkaReceiveBody {
                                topic: m.topic().to_string(),
                                key: match m.key() {
                                    None => None,
                                    Some(k) => match str::from_utf8(k) {
                                        Ok(s) => Some(s.to_string()),
                                        Err(e) => {
                                            warn!("Error while deserializing message key: {:?}", e);
                                            None
                                        }
                                    },
                                },
                                payload: payload.to_string(),
                                timestamp: m.timestamp().to_millis(),
                            })
                            .to_string(),
                        )
                        .send()
                        .await
                    {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error while sending message to receiver: {:?}", e)
                        }
                    }
                    match consumer.commit_message(&m, CommitMode::Async) {
                        Ok(_) => {}
                        Err(e) => {
                            error!("Error while committing message: {:?}", e)
                        }
                    }
                }
            };
        }
    }
}

#[tokio::main]
async fn main() {
    match option_env!("CONSOLE_LOG") {
        Some(_) => {
            pretty_env_logger::init();
        }
        None => {
            Builder::with_level(get_env_level().as_str())
                .with_target_writer("*", new_writer(tokio::io::stdout()))
                .init();
        }
    }

    let cli = Cli::parse();

    match &cli.command {
        Commands::Sender(sender) => {
            info!("Running sender with args: {:?}", sender);
            sender.consume_and_send().await;
        }
        Commands::Receiver(receiver) => {
            info!("Running receiver with args: {:?}", receiver);
            receiver.receive_and_produce().await;
        }
    }
}
