# Docker Deployment Guide

This directory contains Docker configurations for deploying ServerStatus-Rust components.

## Directory Structure

```
docker/
├── server/              # Server-side deployment
│   ├── Dockerfile       # Production server image
│   ├── Dockerfile.cloud # Cloud server variant
│   └── docker-compose.yml
├── client/              # Client-side deployment
│   ├── Dockerfile       # Client image
│   ├── docker-compose.yml
│   ├── vnstat.conf      # Traffic statistics configuration
│   └── .env.example     # Environment variables template
└── README.md            # This file
```

## Server Deployment

### Prerequisites

- `config.toml` file in the repository root directory
- `stats.json` file in the repository root directory (create if not exists)

### Quick Start

```bash
# From repository root, create stats.json if it doesn't exist
touch stats.json

# Start the server
docker-compose -f docker/server/docker-compose.yml up -d
```

Or from the server directory:

```bash
cd ../..
touch stats.json
cd docker/server
docker-compose up -d
```

The server will be available at `http://localhost:8080`.

### Services

- **stat_server**: Main monitoring server
- **Ports**: 8080 (HTTP), 9394 (Prometheus metrics)

### Building Server Image

```bash
# From repository root
docker build -f docker/server/Dockerfile -t serverstatus-server:latest .
```

## Client Deployment

### Platform Support

⚠️ **Note**: Client requires `host` network mode, which is only available on Linux. Docker Desktop (Mac/Windows) does not support `network_mode: host`.

⚠️ **Architecture**: The client Dockerfile currently downloads x86_64 (amd64) binaries only. For ARM64 or other architectures, please use the pre-built releases or compile from source.

### Configuration

1. Copy the environment template:
```bash
cd docker/client
cp .env.example .env
```

2. Edit `.env` with your server details:
```env
SERVER_HOST=https://your-server.example.com
SERVER_PORT=443
GID=your-group-id
PASSWORD=your-password
ALIAS=my-server-name
HOST_TYPE=kvm
IP_SOURCE=ip-api.com
INTERVAL=5
```

### Quick Start

```bash
cd docker/client
docker-compose up -d
```

### Services

- **serverstatus-client**: Monitoring client that reports to the server
- **vnstat**: Traffic statistics daemon (shared data with client)

### Environment Variables

| Variable | Default | Description |
|----------|---------|-------------|
| SERVER_HOST | https://host.example.com | Server address (HTTPS recommended) |
| SERVER_PORT | 443 | Server port |
| GID | group1 | Group ID for authentication |
| PASSWORD | changeme | Password for authentication |
| ALIAS | unknown | Server alias/name displayed on dashboard |
| HOST_TYPE | kvm | Host type (kvm, xen, physical, etc.) |
| IP_SOURCE | ip-api.com | IP detection service |
| INTERVAL | 5 | Reporting interval in seconds |

### Data Persistence

The client uses local volumes for traffic statistics:
- `./vnstat/`: Traffic statistics database (persisted across restarts)

## Building Images

### Build Server Image

```bash
# From repository root
docker build -f docker/server/Dockerfile -t serverstatus-server:latest .
```

### Build Client Image

```bash
cd docker/client
docker build -f Dockerfile -t serverstatus-client:latest .
```

## Logs

View service logs:

```bash
# Server logs
cd docker/server
docker-compose logs -f stat_server

# Client logs
cd docker/client
docker-compose logs -f serverstatus-client
docker-compose logs -f vnstat
```

## Stopping Services

```bash
# Stop server
cd docker/server
docker-compose down

# Stop client
cd docker/client
docker-compose down
```

## Troubleshooting

### Client not reporting to server

1. Verify `.env` file exists and contains correct `SERVER_HOST` and `PASSWORD`
2. Check network connectivity: `docker-compose logs serverstatus-client`
3. Ensure firewall allows outbound connections to server

### vnstat not collecting traffic data

1. Verify `vnstat/` directory exists and is writable
2. Check vnstat logs: `docker-compose logs vnstat`
3. Ensure container has access to host network interfaces (Linux only)

## Notes

- Client requires `host` network mode to properly access network interfaces and host information
- vnstat daemon needs access to host network statistics
- Ensure firewall rules allow communication between client and server
- On non-Linux systems, use the pre-built server image instead of building locally
