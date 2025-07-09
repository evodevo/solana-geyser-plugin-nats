#!/bin/bash

# Solana Geyser Plugin NATS - Docker Test Runner

echo "Solana Geyser Plugin NATS - Integration Test"
echo "=============================================="

# Check if Docker is running
echo "Checking if Docker is running..."
if ! docker info >/dev/null 2>&1; then
    echo "Docker is not running. Please start Docker and try again."
    exit 1
else
    echo "Docker is running"
fi

# Get script directory
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
echo "Running from: $SCRIPT_DIR"

# Clean up existing containers and volumes
echo "Cleaning up existing containers and volumes..."
docker-compose down -v --remove-orphans

# Build and start all services
echo "Building and testing plugin integration..."
echo "Starting Docker build process..."

if docker-compose up --build --abort-on-container-exit; then
    echo ""
    echo "Integration test completed successfully!"
    # Cleanup
    echo ""
    echo "Cleaning up containers..."
    docker-compose down -v
    
    exit 0
else
    echo ""
    echo "Integration test failed!"
    echo ""
    
    # Cleanup
    echo ""
    echo "Cleaning up containers..."
    docker-compose down -v
    
    exit 1
fi 