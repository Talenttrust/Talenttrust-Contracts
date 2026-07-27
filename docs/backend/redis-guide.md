# Redis Developer Guide

This guide explains how Redis is utilized in the TalentTrust Backend, the distinction between real and mocked Redis behavior, and how to handle Redis in different environments (Local, CI, and Testing).

## 1. When Redis is Required

A live Redis instance is required for the following backend features:

- **Queue Processing**: All background jobs, such as payment processing, email notifications, and reputation updates, are handled via Redis-backed queues (e.g., BullMQ).
- **Cache-Backed Behavior**: Distributed caching for performance and rate-limiting to protect API endpoints.
- **Integration Paths**: Any flow that requires persistent background state or shared state across multiple service instances.

### Startup Requirements
The backend service will attempt to connect to Redis during startup. If Redis is unavailable:
- The service may fail to start if the connection is mandatory.
- Queue-dependent features will be disabled or fail immediately.
- Rate limiting may default to a "fail-open" state, potentially exposing the service.

### Identifying Redis-Dependent Features
Developers can identify Redis-dependent features by looking for:
- Usage of `Queue` or `Worker` classes.
- Calls to the `CacheService`.
- Configuration keys starting with `REDIS_` in the environment files.

---

## 2. When Redis Is Mocked

Redis is mocked in specific test scenarios to ensure speed and determinism.

- **Unit Tests**: Logic that depends on Redis but doesn't test the Redis integration itself.
- **Isolated Service Tests**: Testing service-level logic without the overhead of a real network connection.
- **Mocking Library**: We typically use `ioredis-mock` to simulate Redis behavior in-memory.

### Why Mock?
- **Speed**: In-memory mocks are significantly faster than real Redis.
- **Determinism**: Eliminates race conditions and timing issues during unit testing.
- **Isolation**: Tests can run in parallel without sharing state.

### Limitations of Mocked Redis
- **Timing & Latency**: Mocks do not simulate real-world network latency or processing time.
- **Lua Scripts**: Complex Lua scripts may not behave identically to a real Redis server.
- **Persistence**: Mocks do not persist data to disk; data is lost when the test process exits.
- **Hiding Issues**: Mocked tests may pass even if there are serialization errors or command incompatibilities that only appear in real Redis.

---

## 3. How CI Runs Redis

In the Continuous Integration (CI) environment, we prioritize reliability and parity with production.

- **Live Redis Service**: CI uses a live Redis service container (configured in `.github/workflows/`).
- **Test Suites**: Integration and End-to-End (E2E) test suites expect a real Redis instance to be available.
- **Queue Tests**: Queue-related tests in CI run against the live Redis service to verify actual job processing, retries, and timing behavior.

### CI vs Local Failures
If CI passes but local tests fail (or vice versa), check:
- Redis version differences.
- Network latency or resource constraints in the local environment.
- Residual data in the local Redis instance (always flush Redis before running tests).

---

## 4. How to Reproduce Queue Test Behavior Locally

To match local behavior with CI, follow these steps:

### Running Redis Locally
The recommended way to run Redis locally is via Docker:
```bash
docker run -d -p 6379:6379 --name tt-redis redis:7-alpine
```

### Running Tests with Real Redis
To run integration tests against your local Redis instance:
```bash
export USE_REAL_REDIS=true
export REDIS_URL=redis://localhost:6379
npm test
```

### Running Tests with Mocked Redis
By default, unit tests use mocked Redis. To explicitly run with mocks:
```bash
export USE_REAL_REDIS=false
npm test
```

### Reproducing Queue Failures
Queue failures are often related to timing or job visibility. To reproduce:
1. Use a real Redis instance.
2. Slow down your local environment or add artificial delays in workers.
3. Monitor the queue using a tool like `bull-board` or `redis-cli monitor`.

---

## 5. Comparison: Local vs. Mocked vs. CI Redis

| Feature | Local Redis (Docker) | Mocked Redis (ioredis-mock) | CI Redis (Service) |
|---------|----------------------|-----------------------------|--------------------|
| **When to use** | Integration testing, debugging | Unit testing, fast dev loops | Final validation, PR checks |
| **Reproduces** | Network, Persistence, Lua | Basic API, Key/Value logic | Production-like environment |
| **Missing** | Multi-node clusters | Timing, Persistence, Errors | Local resource constraints |
| **Failure Modes** | Port conflicts, Memory | Mock bugs, False positives | Service startup timeouts |

---

## 6. Troubleshooting Notes

### Redis Not Running Locally
- Check if Docker is running: `docker ps`.
- Ensure port `6379` is not occupied by another process: `lsof -i :6379`.

### Queue Jobs Not Being Processed
- Ensure the Worker is started: `npm run worker`.
- Check Redis connectivity: `redis-cli ping`.
- Look for "Stalled Jobs" in the logs; this usually indicates a crash or timeout.

### Tests Passing with Mocks but Failing with Real Redis
- Check for serialization issues (e.g., trying to store non-JSON objects).
- Look for race conditions that the mock hides due to zero latency.
- Ensure the real Redis is flushed before tests: `redis-cli FLUSHALL`.

---

## 7. Security and Reliability Notes

- **Dual Testing Strategy**: Always test queue logic with both mocked and real Redis. Mocks catch logic errors quickly, while real Redis catches integration and timing issues.
- **Timing & Retries**: Production Redis behavior depends on network stability. Ensure your retry logic is tested against a real instance.
- **Assumption Warning**: Never assume that a passing "mocked" test guarantees production behavior. Real Redis has specific persistence and atomicity characteristics that mocks cannot perfectly replicate.
