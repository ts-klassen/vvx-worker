# vvx-mock-bench REST API

                        http://127.0.0.1:8080/api/v1

The vvx-mock-bench (vxmb) benchmark mimics a production-style speech-synthesis evaluation: controllers interact with queued tasks through this API, issue work to engines, and receive a score from the service. Treat the system as a black box; only the behaviour described here is observable, and internal mechanisms are not exposed. The score reported by the service is a single floating-point value where lower numbers represent better performance.

Base URL `/api/v1`. All bodies JSON. Errors return HTTP 500 with empty body.

`eval_id` identifies the namespace created for a client. `engine_id` is a zero-based index running from `0` through `engine_count - 1`, allowing multiple engines to be addressed independently. `speaker_id` indexes speakers, and `task_id` references work issued by the queue.

## POST `/api/v1/evaluations`
Create a namespace. Use this to start a fresh benchmark run and obtain the opaque `eval_id` plus a snapshot of runtime configuration.

201 body: `{ "eval_id": "...", "config": {...} }`

## POST `/api/v1/evaluations/{eval_id}/tasks`
Fetch ready tasks. Each call returns the tasks that have been released for the namespace so your controller can schedule them; nothing else about task timing is revealed.

200 body: `{ "tasks": [{ "task_id": "...", "speaker_id": 0 }] }`

Blocks internally until tasks are available.
Returns an empty array once the queue is exhausted.
The `tasks` array may contain any number of entries (including zero). Each `task_id` is issued only once.

Clients should continue calling this endpoint until it returns an empty array to ensure all work has been collected.

## PUT `/api/v1/evaluations/{eval_id}/engines/{engine_id}/speaker`
Set engine speaker. Call this when an engine must operate on a specific speaker before synthesising tasks for that speaker.

Request body: `{ "speaker_id": 0 }`
202 body: `{}`

Blocks internally while applying overhead.
Engine-level requests are processed sequentially; do not issue concurrent `/speaker` commands for the same `engine_id`.

## POST `/api/v1/evaluations/{eval_id}/engines/{engine_id}/synthesis`
Complete a task. Submit one of the fetched tasks to the engine using the referenced speaker; the server records completion and enforces any simulated processing delay.

Request body: `{ "speaker_id": 0, "task_id": "..." }`
202 body: `{}`

Blocks internally for task latency.
If the `task_id` was never fetched or the `speaker_id` does not match the current engine assignment, the request fails with a 500 error.

## GET `/api/v1/evaluations/{eval_id}/metrics`
Get score. This reflects the benchmark’s current assessment of your run based on tasks the service has accepted.

200 body: `{ "score": 12345.67 }`

The endpoint is available throughout the run; calling it does not close the evaluation.

## GET `/api/v1/config`
Read defaults. Consult this to learn the benchmark’s standard configuration without creating a namespace.

200 body: `{ "task_count": 100000, "engine_count": 4, "speaker_count": 255 }`

## General behaviour
- Evaluations created with `POST /evaluations` are isolated; you may operate multiple namespaces in parallel by storing each `eval_id`.
- All endpoints that include a JSON body require the `Content-Type: application/json` header.
- Requests may keep the HTTP connection open for several seconds to model real-world processing; design clients to tolerate long-lived responses.
- Each engine processes one blocking operation at a time. Await each 202 response before issuing the next command to that engine.
