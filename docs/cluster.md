# Cluster Mode - Distributed ESP32 Device Management

ESPBrew Cluster provides distributed ESP32 device management similar to build farm systems. It enables parallel flashing across multiple worker nodes, device pooling, and job queuing.

## Architecture

The cluster uses a master/worker architecture:

```
                    Master Node (Port 8081)
                    /        |        \
                   v         v         v
              Worker 1   Worker 2   Worker 3
              /  |  \    /  |  \    /  |  \
            ESP1 ESP2 ESP3 ESP4 ESP5 ESP6 ESP7 ESP8 ESP9
```

## Features

- **Master/Worker Architecture**: Central coordinator with distributed execution nodes
- **Device Pooling**: Automatic device discovery and reservation management
- **Job Queue**: Centralized job queue with intelligent device-to-job assignment
- **WebSocket Communication**: Real-time messaging between master and workers
- **mDNS Discovery**: Automatic node discovery on local network
- **Device Reservation**: Lock-based device management with timeout handling
- **Fault Tolerance**: Automatic cleanup of expired nodes and reservations

## Cluster Commands

### Start Master Node

```bash
# Start cluster master (interactive, Ctrl+C to stop)
espbrew cluster --name my-cluster --role master start

# Start master on specific bind address
espbrew cluster --name my-cluster --role master start --bind 192.168.1.100:8081
```

### Start Worker Node

```bash
# Start cluster worker
espbrew cluster --name my-cluster --role worker start

# Worker will auto-discover master via mDNS
```

### Join Specific Master

```bash
# Start worker and join specific master
espbrew cluster --name my-cluster --role worker join --master 192.168.1.100:8081
```

### Cluster Status

```bash
# Show cluster status
espbrew cluster --name my-cluster status

# Watch for changes
espbrew cluster --name my-cluster status --watch
```

Output example:
```
Cluster: my-cluster
Nodes: 2

Nodes:
  master@citera-773.local (master)
    Address: 192.168.1.100:8081
    Devices: 0
    Capabilities: flash, monitor
  worker@citera-773.local (worker)
    Address: 192.168.1.101:8081
    Devices: 2
    Capabilities: flash, monitor
```

### Cluster Topology

```bash
# Show cluster topology
espbrew cluster --name my-cluster topology
```

### List Nodes

```bash
# List all nodes in cluster
espbrew cluster --name my-cluster nodes
```

### List Devices

```bash
# List all devices in cluster
espbrew cluster --name my-cluster devices
```

## Node Roles

### Master Node

Responsibilities:
- Job scheduling and queue management
- Device pool and reservation management
- State aggregation and status reporting
- Worker coordination

Configuration:
```bash
espbrew cluster --name my-cluster --role master start --bind 0.0.0.0:8081
```

### Worker Node

Responsibilities:
- Local device management (USB detection)
- Job execution (flash, monitor, reset)
- Progress reporting to master
- Device announcements

Configuration:
```bash
espbrew cluster --name my-cluster --role worker start
```

## Device Reservation

The cluster uses a reservation system to prevent concurrent device access:

1. Job submitted to master
2. Master reserves device in pool
3. Job dispatched to worker
4. Worker executes job
5. Reservation released on completion
6. Stale reservations cleaned up by timeout

## Discovery

Nodes discover each other using mDNS (Multicast DNS):

- Service type: `_espbrew-cluster._tcp.local.`
- Default port: 8081
- TXT records include: cluster name, role, device count, capabilities

## Use Cases

### CI/CD Pipelines

```bash
# Start cluster with multiple workers
# Server 1 (Master)
espbrew cluster --name test-cluster --role master start

# Server 2 (Worker with devices)
espbrew cluster --name test-cluster --role worker start

# Server 3 (Worker with devices)
espbrew cluster --name test-cluster --role worker start
```

### Hardware-in-the-Loop Testing

Distribute tests across multiple physical devices for parallel execution.

### Remote Lab Management

Manage devices across different locations with central coordination.

### Mass Provisioning

Flash firmware to multiple devices in parallel.

## Configuration

### Cluster Name

All nodes in a cluster must use the same cluster name:

```bash
espbrew cluster --name production --role master start
espbrew cluster --name production --role worker start
```

### Bind Address

Default bind address is `0.0.0.0:8081`. Customize with:

```bash
espbrew cluster --name my-cluster --role master start --bind 192.168.1.100:8081
```

### Device Auto-Detection

Workers automatically detect USB ESP32 devices on startup:

- ESP32 (FTDI: 0x0403)
- ESP32 (CP210x: 0x10C4)
- ESP32 (CH341: 0x1A86)
- ESP32-S3 built-in USB (0x303A)

## Troubleshooting

### Nodes Not Discovering Each Other

- Check mDNS is enabled on your network
- Verify nodes use the same cluster name
- Check firewall rules for port 8081 and mDNS (5353)

### Device Reservation Issues

- Check device availability: `espbrew cluster --name my-cluster devices`
- Verify worker has detected devices
- Check for stale reservations (auto-cleanup after 5 minutes)

### Connection Issues

- Verify master is running: `espbrew cluster --name my-cluster nodes`
- Check network connectivity between nodes
- Review logs for connection errors
