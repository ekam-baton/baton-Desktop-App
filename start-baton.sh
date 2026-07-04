docker-compose down
docker-compose up -d postgres redis
echo "Waiting for Postgres and Redis to be ready..."
sleep 5
docker-compose up --build -d mcp-connector a2a-router
docker-compose logs -f
