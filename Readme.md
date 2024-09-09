### Compile
```shell
docker build -t gaia-hub .
```

### Init db
```shell
./init.sh
```

### Run
```shell

cat <<EOF > .env
LOG_FILE=/logs/gaia-hub.log
DATABASE_URL=/data/gaia-domain.db
REDIS_URL=redis://:@redis:6379/0
SERVER_HOST=0.0.0.0
SERVER_PORT=1337
DB_POOL_SIZE=20
DB_POOL_MIN_SIZE=5
EOF

docker run -d --name gaia-hub --env-file .env -v ./data:/data -v ./logs:/logs -p 1337:1337 --restart=always gaia-hub
```
