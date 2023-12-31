# kafka-transceiver-rs

`kafka-transceiver-rs` is a versatile Kafka message handling service written in Rust. It is designed to function as both a Sender and a Receiver, depending on its configuration.

## Features

1. **Sender Functionality**: The Sender consumes messages from a Kafka Topic and forwards them via HTTP requests to a Receiver service.
2. **Receiver Functionality**: The Receiver exposes a `/kafka-receive` endpoint to accept HTTP requests from the Sender. Upon receiving a message, it forwards the message to its own Kafka instance.
3. **Dual Role**: Both the Sender and Receiver functionalities are encapsulated within the same program. The role it plays (Sender or Receiver) is determined by its configuration.
4. **Independent Kafka Instances**: The Sender and Receiver each interact with their own separate Kafka instances.

This project offers a compact and efficient solution for Kafka message transmission, making it a valuable tool for distributed systems.

## Startup

See `help` for more information.

```bash
$ cargo run -- help
```

### Sender

```bash
# Run help
$ cargo run -- sender --help
# Run service
$ cargo run -- sender --kafka-address localhost:9092 --kafka-topic test_topic --kafka-group_id=sender-consumer-group --receiver-endpoint localhost:8080/kafka-receive
```

### Receiver

```bash
# Run help
$ cargo run -- receiver --help
# Run service
$ cargo run -- receiver --kafka-address localhost:9092 --port 8080
```

## Architecture

```
+-----------------+          +-----------------+
|                 |          |                 |
| +-------------+ |          | +-------------+ |
| |             | |          | |             | |
| |    Kafka    | |          | |    Kafka    | |
| |             | |          | |             | |
| +-------------+ |          | +-------------+ |
|        |        |          |        ^        |
|        |        |          |        |        |
|    recv msg     |          |    send msg     |
|        |        |          |        |        |
|        v        |          |        |        |
| +-------------+ |          | +-------------+ |
| |             | |          | |             | |
| |  TRX-Sender |-----http---->| TRX-Receiver| |
| |             | |          | |             | |
| +-------------+ |          | +-------------+ |
|                 |          |                 |
+-----------------+          +-----------------+
 Internal-Network              External-Network
```