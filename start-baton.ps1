docker-compose down
docker-compose up -d postgres redis
Write-Host "Waiting for Postgres and Redis to be ready..."
Start-Sleep -Seconds 5
docker-compose up --build -d mcp-connector a2a-router
docker-compose logs -f
