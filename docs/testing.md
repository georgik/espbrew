# Testing Guide

This guide covers testing ESPBrew functionality, including the cluster mode and various testing scenarios.

## Test Mode for Cluster

For testing cluster functionality without manual intervention, use the `--test-duration` flag:

```bash
# Start master in test mode (auto-shutdown after 2 seconds)
espbrew cluster --name test-cluster --role master start --test-duration 2

# Start worker in test mode
espbrew cluster --name test-cluster --role worker start --test-duration 2
```

This is useful for:
- Automated testing
- CI/CD integration
- Verifying cluster startup
- Testing mDNS announcement

## Unit Tests

Run the unit test suite:

```bash
# Run all tests
cargo test

# Run tests for specific module
cargo test --lib cluster::reservation

# Run tests with output
cargo test -- --nocapture

# Run tests in parallel
cargo test -- --test-threads=4
```

## Integration Tests

ESPBrew includes integration tests for cluster functionality:

```bash
# Run integration tests only
cargo test --test integration

# Run specific test
cargo test test_cluster_node_expiry_and_cleanup
```

## Manual Testing

### Device Discovery

Test USB device detection:

```bash
# List all USB devices
espbrew boards

# Verify ESP32 devices are detected
ls /dev/ttyUSB*  # Linux
ls /dev/tty.usb*  # macOS
```

### Project Detection

Test project type detection:

```bash
# ESP-IDF project
cd /path/to/esp-idf-project
espbrew --cli

# Rust project
cd /path/to/rust-esp32-project
espbrew --cli

# Arduino project
cd /path/to/arduino-project
espbrew --cli
```

### Flashing Test

Test flashing with a known-good binary:

```bash
# Build first
espbrew --cli build

# Flash to device
espbrew --cli flash --port /dev/ttyUSB0

# Monitor output
espbrew --cli monitor --port /dev/ttyUSB0 --timeout 10
```

## Cluster Testing

### Single Machine Testing

Test master and worker on same machine:

```bash
# Terminal 1: Start master
espbrew cluster --name test --role master start --test-duration 30

# Terminal 2: Start worker
espbrew cluster --name test --role worker start --test-duration 30

# Terminal 3: Check status
espbrew cluster --name test status
```

### Multi-Machine Testing

Test across multiple machines:

1. Start master on first machine:
```bash
espbrew cluster --name test --role master start --bind 192.168.1.100:8081
```

2. Start worker on second machine:
```bash
espbrew cluster --name test --role worker join --master 192.168.1.100:8081
```

3. Verify cluster status from any machine:
```bash
espbrew cluster --name test nodes
```

### mDNS Discovery Testing

Test mDNS service discovery:

```bash
# Discover all ESPBrew cluster nodes
dns-sd -B _espbrew-cluster._tcp local.

# Or use avahi-browse on Linux
avahi-browse --terminate _espbrew-cluster._tcp
```

## Test Coverage

Current test coverage by module:

| Module | Tests | Coverage |
|-------|-------|----------|
| cluster::messaging | 15 | Core types |
| cluster::state | 12 | State management |
| cluster::master | 18 | Job scheduling |
| cluster::worker | 10 | Job execution |
| cluster::network | 8 | WebSocket handling |
| cluster::discovery | 8 | mDNS discovery |
| cluster::backends | 6 | Device backends |
| cluster::node | 8 | Node lifecycle |
| cluster::reservation | 13 | Device pool |
| **Total** | **104** | **Growing** |

## CI/CD Testing

For CI/CD pipelines, use test mode for cluster:

```yaml
# Example GitHub Actions
- name: Start cluster master
  run: espbrew cluster --name ci-test --role master start --test-duration 60

- name: Start cluster worker
  run: espbrew cluster --name ci-test --role worker start --test-duration 60

- name: Verify cluster
  run: espbrew cluster --name ci-test nodes
```

## Benchmark Tests

Run performance benchmarks:

```bash
# Run all benchmarks
cargo test --release --benches

# Run specific benchmark
cargo test --bench espbrew_benchmarks -- --nocapture
```

## Debugging Tests

### Enable Logging

```bash
# Debug logging during tests
RUST_LOG=debug cargo test

# Trace logging
RUST_LOG=trace cargo test

# Specific module logging
RUST_LOG=espbrew::cluster=debug cargo test
```

### Backtrace

```bash
# Get full backtrace on test failure
RUST_BACKTRACE=1 cargo test

# Full backtrace with source lines
RUST_BACKTRACE=full cargo test
```

## Common Issues

### Tests Failing with "Device Not Found"

Tests may try to access physical devices. Most tests mock devices or handle missing devices gracefully.

### mDNS Tests Failing

mDNS tests may fail in containerized environments. They're designed to pass on systems with full network access.

### Port Already in Use

Previous test may have left a port bound. Wait a few seconds or kill the process:
```bash
lsof -ti:8081 | xargs kill -9  # macOS/Linux
```

## Test Development

### Adding New Tests

When adding cluster functionality, include tests in the module's `tests` module:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_functionality() {
        // Test implementation
    }

    #[tokio::test]
    async fn test_async_functionality() {
        // Async test implementation
    }
}
```

### Test Naming Convention

Use descriptive test names:
- `test_<function>_<scenario>` for unit tests
- `test_<module>_<feature>` for integration tests
- `test_<error_case>` for negative tests
