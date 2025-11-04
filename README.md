vvx-worker
===========

Usage
-----
- Start RabbitMQ (and the VXMB API if you plan to run in mock mode).
- Launch workers with their engine IDs. Workers default to VOICEVOX mode; add `--mock` to keep the HTTP mock behaviour:
  - Real VOICEVOX: `cargo run --bin worker -- 0 --voicevox-dict /path/to/open_jtalk_dic --voicevox-model-dir /path/to/models`
  - Mock API: `cargo run --bin worker -- 0 --mock`
  - You can also provide `ENGINE_ID=1 cargo run --bin worker` (flags go after `--`).
- Run the client:
  - VOICEVOX synthesis: `cargo run --bin client -- --speaker-id 1 --text "こんにちは" --output-dir ./output`
  - Mock evaluation workflow: `cargo run --bin client -- --mock`

Configuration
-------------
Environment variables override defaults:
- `AMQP_ADDR` – AMQP URL (default `amqp://guest:guest@127.0.0.1:5672/%2f`)
- `VXMB_API` – REST API base (default `http://127.0.0.1:8080/api/v1`)
- `TASK_QUEUE` – queue name for tasks (default `vvx_tasks`)
- `RESULT_EXCHANGE` – exchange name for task results (default `vvx_results`)
- `VOICEVOX_ORT_LIB` – path to the ONNX Runtime shared library (optional; used when running workers without `--mock`)
- `VOICEVOX_OPEN_JTALK_DIR` – Open JTalk dictionary directory (required for real VOICEVOX mode if `--voicevox-dict` is omitted)
- `VOICEVOX_MODEL_DIR` – directory containing VOICEVOX model assets (required for real VOICEVOX mode if `--voicevox-model-dir` is omitted)
