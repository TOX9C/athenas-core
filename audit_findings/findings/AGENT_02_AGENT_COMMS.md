# Audit Findings — Agent Comms, Swarm, Notification, and Tests

## Summary
Deep-dive analysis of the following files:
- `crates/athena-core/src/agent_comms.rs`
- `crates/athena-core/src/swarm.rs`
- `crates/athena-core/src/notification.rs`
- `crates/athena-core/src/tests.rs`

---

## 1. Concurrency Issues

### 1.1 Deadlock via Lock Ordering Inversion in `disconnect_agent`
- **File:** `agent_comms.rs`
- **Lines:** 227-247
- **Category:** Concurrency
- **Severity:** High
- **Description:** `disconnect_agent` acquires the `sessions` lock, drops it, then reacquires it. While the lock is dropped, another thread could modify the map, causing the second acquisition to operate on stale data. More critically, if another method acquires `sessions` and `pending_input` in a different order, deadlocks can occur.
- **Impact:** Inconsistent state, potential deadlocks under contention.
- **Suggested Fix:** Perform the `remove` in a single lock acquisition. Check for the key inside one `lock()` scope, and if present, remove directly.

```rust
pub fn disconnect_agent(&self, agent_id: &str) -> Result<bool, AgentCommsError> {
    let mut sessions = self.sessions.lock().map_err(|_| AgentCommsError::LockPoisoned)?;
    let entry = sessions.values().find(|s| s.session.agent_id == agent_id).map(|s| s.session.id.clone());
    if let Some(id) = entry {
        sessions.remove(&id);
        Ok(true)
    } else {
        Ok(false)
    }
}
```

### 1.2 Stale `watch_rx` after All `watch_tx` Senders Dropped
- **File:** `swarm.rs`
- **Lines:** 116-130
- **Category:** Concurrency
- **Severity:** Medium
- **Description:** `SwarmCoordinator` stores `watch_tx: Option<watch::Sender<SwarmState>>` and `watch_rx: watch::Receiver<SwarmState>`. `watch_tx` uses an `Option` to allow `Clone` to derive itself, but `watch_tx` is not used consistently. When `watch_tx` is cloned, the receivers can still St-like see a stale default `SwarmState` because no sender ever pushes an update after the initial default.
- **Impact:** Subscribers (`subscribe()`) may receive the default `SwarmState` forever if no new state is sent.
- **Suggested Fix:** Ensure `watch_tx` is always used to push updates after the initial `watch_state` loop. Alternatively, initialize with a meaningful state rather than `SwarmState::default()`.

### 1.3 `RwLock` Underneath `std::sync::Mutex` in Notification Service
- **File:** `notification.rs`
- **Lines:** 109-115 (field declarations)
- **Category:** Concurrency
- **Severity:** Medium
- **Description:** `NotificationService` uses `Arc<RwLock<Vec<...>>>` for history. While this allows multiple readers, any contention on the `RwLock` from other parts of the system could cause writer starvation because `push_notification` and other mutating methods take the write lock. Given `NotificationService` is on the hot path (receiving notifications from many agents), heavy write contention can degrade performance.
- **Impact:** Latency spikes for notification operations under load.
- **Suggested Fix:** Consider a lock-free structure like `crossbeam::queue`, or at least profile under load. Alternatively, batch notifications rather than writing per-notification.

---

## 2. Message Passing Bugs

### 2.1 Unbounded Channel for Agent Communication
- **File:** `agent_comms.rs`
- **Lines:** 368-370
- **Category:** Message Passing / Resource Management
- **Severity:** Medium
- **Description:** `handle_connection` uses `std::sync::mpsc::channel::<Vec<u8>>()` (unbounded) to send bytes to each agent. An unbounded channel will grow without limit if the consumer (agent thread) is slow or dead.
- **Impact:** Memory exhaustion (OOM) if agents cannot keep up with messages.
- **Suggested Fix:** Switch to a bounded channel (e.g. `sync_channel` with a reasonable limit like 1024 messages) and handle the `TrySendError::Full` case by dropping the oldest message or disconnecting the slow agent.

```rust
let (tx, rx) = std::sync::mpsc::sync_channel::<Vec<u8>>(1024);
```

### 2.2 `respond_to_input_request` Sender Cannot Detect Cancelled Request
- **File:** `agent_comms.rs`
- **Lines:** 241-255
- **Category:** Message Passing / Logic Error
- **Severity:** Medium
- **Description:** `respond_to_input_request` removes and sends on a `SyncSender`. If the receiving end (in `handle_request_input`) has already returned (e.g., due to a timeout or the agent disconnecting), `send` will return `Err` but `respond_to_input_request` maps this to a generic `BrokenPipe` without any retry or notification to the caller that the request was already cancelled.
- **Impact:** Caller cannot distinguish between a successful unblock and a failed send to a dead receiver.
- **Suggested Fix:** Return a descriptive error or an enum variant indicating the receiver was already dropped.

### 2.3 `handle_request_input` Blocks Indefinitely if No Response
- **File:** `agent_comms.rs`
- **Lines:** 526-583
- **Category:** Message Passing / Resource Management
- **Severity:** High
- **Description:** `handle_request_input` blocks on `input_rx.recv()` with a `sync_channel(1)` and no timeout. If the frontend never sends a response, the agent thread (and by extension, the stream handler thread) is blocked forever. The `cancel_input_request` method only removes the sender but cannot unblock the already-waiting receiver.
- **Impact:** Resource leak (thread stuck waiting), unresponsive agent threads.
- **Suggested Fix:** Use `recv_timeout` or a select with a cancellation token. Alternatively, store a handle to abort the waiting thread or use an async channel.

---

## 3. Resource Management

### 3.1 Leaked TCP Stream Writer Thread on Connection Close
- **File:** `agent_comms.rs`
- **Lines:** 381-393
- **Category:** Resource Management
- **Severity:** Medium
- **Description:** The spawn-writer thread reads from the `rx` channel. When the connection is cleaned up (`cleanup_connection`) and `stream` is dropped, there is no guarantee the writer thread ever terminates—`rx` is never closed by the main loop.
- **Impact:** Thread leak every time an agent disconnects unexpectedly.
- **Suggested Fix:** Drop or close `tx` (sender) variable in `cleanup_connection` or after the read loop so the writer thread receives the disconnect/termination signal.

### 3.2 `cancel_input_request` Does Not Notify Receiver
- **File:** `agent_comms.rs`
- **Lines:** 256-267
- **Category:** Resource Management
- **Severity:** Medium
- **Description:** Removing the `SyncSender` from `pending_input` does not unblock the waiting receiver. The thread in `handle_request_input` remains stuck on `input_rx.recv()`.
- **Impact:** Thread leak and system resource exhaustion over repeated cancel/timeout cycles.
- **Suggested Fix:** Use a oneshot or refcounted cancellation token. When cancelling, signal the token so the waiter can return. Alternatively, drop the `rx` by design so `recv()` returns immediately.

### 3.3 Unbounded Thread Spawn per Connection
- **File:** `agent_comms.rs`
- **Lines:** 350-363
- **Category:** Resource Management
- **Severity:** Medium
- **Description:** `init_agent_comms` spawns one thread for the accept loop and a new thread per connection. With no rate limiting or thread pool, a burst of agent connections could spawn thousands of OS threads.
- **Impact:** Exhaustion of OS thread limits; unbounded memory growth.
- **Suggested Fix:** Use a thread pool (e.g. `rayon` or `tokio` runtime) to limit concurrent connections, or accept using async I/O.

---

## 4. Logic Errors in Swarm Coordination

### 4.1 `watch_state` Loop Does Not Re-read After File Change
- **File:** `swarm.rs`
- **Lines:** 237-345
- **Category:** Logic Error
- **Severity:** Medium
- **Description:** The `watch_state` loop reads from a static path (`.ade/swarm-state.json`). It does NOT use a file watcher (e.g. `notify` crate); it polls every 5 seconds. Contrastingly, the name `watch_state` implies a reactive watch. Also, the `since_time` of change is derived from `last_action_at` in the file, but the file is always rewritten by this loop itself—creating a tautology.
- **Impact:** Swarm state updates have a 5-second latency, and the loop competes with external writers.
- **Suggested Fix:** Use a real file watcher (e.g. `tokio::fs::watch` or the `notify` crate) or keep state in memory with a lock rather than a file.

### 4.2 `write_state` Is Never Actually Called After `watch_state` Loop Updates
- **File:** `swarm.rs`
- **Lines:** 290-320
- **Category:** Logic Error
- **Severity:** Medium
- **Description:** Inside the loop, when stalled agents are detected, it writes directly with `fs::File::create`, `write_all`, `fs::rename` instead of calling `self.write_state(dir, ...)`. This duplicates the atomic-write logic and bypasses the encapsulated `write_state` method.
- **Impact:** Code duplication, error handling divergence, potential for inconsistent atomic behavior.
- **Suggested Fix:** Refactor the write logic into a single `write_state` call.

### 4.3 `watch_state` Possible TOCTOU / Partial Write Race
- **File:** `swarm.rs`
- **Lines:** 290-320
- **Category:** Logic Error
- **Severity:** Medium
- **Description:** If another process writes `swarm-state.json` between the read at the top of the loop and the write at the bottom, the new data is simply overwritten. The advisory lock used in `send_message` is NOT used here.
- **Impact:** Lost agent state updates when two coordinators or external editors run concurrently.
- **Suggested Fix:** Acquire the same `.lock` file before reading and writing, or use a cross-process mutex.

---

## 5. Security Issues

### 5.1 Plain-Text Token in Memory and No Rotation
- **File:** `agent_comms.rs`
- **Lines:** 88, 264
- **Category:** Security
- **Severity:** Medium
- **Description:** The `token` is a UUID generated once and stored as a `String`. There is no rotation, expiry, or scoping per client. If the token is leaked (e.g., via logs or process memory), any process on the local machine can authenticate.
- **Impact:** Unauthorized local agents could connect and receive/send messages.
- **Suggested Fix:** Support token rotation, assign tokens per session, or use OS-level authentication (e.g. checking peer PID / user).

### 5.2 `Incoming` Stream Allows Unlimited Data — Read Loop Ignores Message Size
- **File:** `agent_comms.rs`
- **Lines:** 396-407
- **Category:** Security
- **Severity:** Medium
- **Description:** `handle_connection` reads lines from `BufReader` with no size limit. A malicious agent could send an single extremely long line (or many long lines) causing unbounded memory growth in the read buffer.
- **Impact:** Potential denial of service via memory exhaustion.
- **Suggested Fix:** Limit the maximum line length (e.g., 64KB or 1MB) and disconnect the client if exceeded.

### 5.3 No Rate Limiting on Connection Acceptance
- **File:** `agent_comms.rs`
- **Lines:** 350-363
- **Category:** Security
- **Severity:** Medium
- **Description:** As noted in resource management, unlimited threads + unlimited connections = easy DoS from a malicious local process.
- **Impact:** Denial of service.
- **Suggested Fix:** Limit total connections and implement rate limiting at the accept level.

---

## 6. Error Handling

### 6.1 Silent Ignores of Deserialization Errors in `handle_incoming_message`
- **File:** `agent_comms.rs`
- **Lines:** 418-421
- **Category:** Error Handling
- **Severity:** Medium
- **Description:** `Err => continue;` silently discards any invalid JSON without logging, telemetry, or disconnecting the sender. Malformed messages are ignored, which makes debugging hard.
- **Impact:** Hidden bugs, silent protocol violations.
- **Suggested Fix:** Log the error and consider disconnecting the peer after a threshold of bad messages.

### 6.2 `send_to_socket` Silently Drops Write Errors
- **File:** `agent_comms.rs`
- **Lines:** 334-339
- **Category:** Error Handling
- **Severity:** Low
- **Description:** `let _ = w.write_all(buf.as_bytes());` ignores any write errors.
- **Impact:** Agent may think a message was sent when actually it failed.
- **Suggested Fix:** Return `Result<(), std::io::Error>` and propagate the error upward.

### 6.3 `now_ms()` Uses `unwrap_or_default()` Instead of Handling Time Errors
- **File:** `agent_comms.rs`
- **Lines:** 135-139
- **Category:** Error Handling
- **Severity:** Low
- **Description:** `now_ms()` silently returns 0 if system time is somehow before the UNIX epoch.
- **Impact:** Timestamps become zero, making activity-based checks (like stale-session cleanup) unreliable.
- **Suggested Fix:** `unwrap_or_default()` is acceptable for `SystemTime`, but document the behavior or panic in development mode if `SystemTime::now()` is ever before the epoch.

---

## 7. Test Coverage Gaps and Test Smells

### 7.1 No Concurrency Stress Tests for `AgentComms`
- **File:** `tests.rs`
- **Lines:** (agenda section around line ~490 and beyond)
- **Category:** Test Coverage
- **Severity:** High
- **Description:** `agent_comms_tests` only tests empty / happy path methods. There are ZERO tests for actual TCP communication, concurrent agents, lock poisoning simulation, or message passing correctness.
- **Impact:** The most critical functionality (TCP comms) is purely undetected in the test suite.
- **Suggested Fix:** Add integration tests using `std::net::TcpStream` that spin up `init_agent_comms`, connect, authenticate, send messages, handle input requests, and disconnect.

### 7.2 `test_watch_prevents_duplicates` Only Sleeps; No Behavior Verification
- **File:** `tests.rs`
- **Lines:** ~850-858
- **Category:** Test Smell
- **Severity:** Medium
- **Description:** The test starts `watch_state` twice and asserts it doesn't crash, but does NOT verify that only one background task is running (e.g., by checking the `watching_dirs` set or counting spawned tasks).
- **Impact:** Missing regression protection if duplicate watch tasks are spawned.
- **Suggested Fix:** After calling `watch_state` twice, read `watching_dirs` to verify it only contains one entry, and verify that only one `CancellationToken` is stored.

### 7.3 No Test for Stalled Agent Logic in `SwarmCoordinator`
- **File:** `tests.rs`
- **Lines:** (swarm_tests section)
- **Category:** Test Coverage
- **Severity:** High
- **Description:** There is no test that writes an agent with an old `last_action_at` to the state file and verifies the `watch_state` loop flags it as `stalled`.
- **Impact:** The core business logic for stall detection (the primary reason for the polling loop) is untested.
- **Suggested Fix:** Write a test that creates a stale state file, calls `watch_state`, waits 5+ seconds, and asserts that the state is updated and the emitter is called with the correct event.

### 7.4 No Lock Poisoning Tests for `NotificationService`
- **File:** `tests.rs`
- **Category:** Test Coverage
- **Severity:** Low
- **Description:** Despite `NotificationService` having explicit error paths for lock poisoning, no test simulates a poisoned lock.
- **Impact:** Error-handling branches are uncovered.
- **Suggested Fix:** Use a custom `RwLock` that panics on read, and verify that the service returns error or empty values correctly.

### 7.5 `send_to_agent_not_found` Tests Error Type but Not Actual Delivery
- **File:** `tests.rs`
- **Lines:** ~630-638
- **Category:** Test Smell
- **Severity:** Low
- **Description:** Tests only verify that `AgentNotFound` is returned for a non-existent agent, not that messages are ever actually sent over a real channel to a real session.
- **Impact:** The actual wire protocol is untested.
- **Suggested Fix:** See 7.1; add integration tests with a real TCP connection.

---

## Architecture / Design Notes

### Event Emitter Pattern Inconsistency
- **File:** All three services (`agent_comms.rs`, `notification.rs`, `swarm.rs`)
- **Observation:** Each service duplicates the same `event_emitter: Arc<Mutex<Option<Box<dyn Fn(...) + Send + Sync>>>>` pattern. This is a lot of boilerplate and introduces a `Mutex` on every emit path (which is expensive for high-frequency events). Consider a shared `EventBus` using `tokio::sync::broadcast` or `crossbeam::channel` that all services can publish to, removing the per-service `Mutex<dyn Fn>`.

### `AgentComms` to `SwarmCoordinator` Integration Gap
- **Observation:** `AgentComms` and `SwarmCoordinator` are entirely separate. There is no mechanism for an agent to automatically appear in the swarm state when it registers via TCP. This seems like a missing integration point for a product called "Swarm".

### Token in `SwarmCoordinator` is Missing
- **Observation:** `SwarmCoordinator` has no authentication at all for mailbox or state operations. Any code with filesystem access can manipulate the `.ade` state.

---

## Final Risk Rating

| Category                         | Count | High | Medium | Low |
|-----------------------------------|-------|------|--------|-----|
| Concurrency                       | 3     | 1    | 2      | 0   |
| Message Passing                   | 3     | 1    | 2      | 0   |
| Resource Management               | 3     | 0    | 3      | 0   |
| Logic Errors (Swarm)              | 3     | 0    | 3      | 0   |
| Security                          | 3     | 0    | 3      | 0   |
| Error Handling                    | 3     | 0    | 2      | 1   |
| Test Coverage / Smells              | 5     | 2    | 2      | 1   |

**Total Findings:** 22
**Recommended Priority Fixes:**
1. `handle_request_input` blocking on `recv()` indefinitely (`AgentComms` — High)
2. Lack of concurrency/stress tests for TCP comms (`tests.rs` — High)
3. Stale `watch_rx` / `watch_tx` semantics in `SwarmCoordinator` (Medium)
4. Unbounded channel memory growth in `AgentComms` (Medium)
5. Thread leak in writer and request cancellation (Medium)
