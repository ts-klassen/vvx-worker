vvx-worker
===========

Usage
-----
- Start RabbitMQ and the VXMB API locally.
- Launch one or more workers with their engine IDs:
  - `cargo run --bin worker -- 0`
  - `ENGINE_ID=1 cargo run --bin worker`
- Run the client to create an evaluation and enqueue tasks:
  - `cargo run --bin client`

Configuration
-------------
Environment variables override defaults:
- `AMQP_ADDR` – AMQP URL (default `amqp://guest:guest@127.0.0.1:5672/%2f`)
- `VXMB_API` – REST API base (default `http://127.0.0.1:8080/api/v1`)
- `TASK_QUEUE` – queue name for tasks (default `vvx_tasks`)
- `RESULT_EXCHANGE` – exchange name for task results (default `vvx_results`)
