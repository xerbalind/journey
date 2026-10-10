FROM docker.io/node:26-alpine AS frontend-builder
WORKDIR /app
COPY frontend/package*.json ./
RUN npm install
COPY frontend/ .
RUN npm run build

FROM docker.io/rust:bookworm AS backend-builder
RUN apt-get update && apt-get install -y libpq-dev build-essential pkg-config
WORKDIR /backend
COPY . .
RUN cargo build --release --locked

FROM docker.io/debian:bookworm-slim
RUN apt-get update && apt-get install -y libpq5 ca-certificates && rm -rf /var/lib/apt/lists/*
WORKDIR /backend
COPY --from=backend-builder /backend/target/release/journey-backend /backend/journey-backend
COPY --from=frontend-builder /app/dist /backend/public
EXPOSE 3000
CMD ["./journey-backend"]
