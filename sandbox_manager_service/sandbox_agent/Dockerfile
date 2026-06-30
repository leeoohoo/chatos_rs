FROM mcr.microsoft.com/devcontainers/base:dev-ubuntu-24.04

ENV DEBIAN_FRONTEND=noninteractive
ENV CHATOS_AGENT_HOST=0.0.0.0
ENV CHATOS_AGENT_PORT=49888
ENV CHATOS_WORKSPACE=/workspace

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      bash \
      ca-certificates \
      cargo \
      cmake \
      curl \
      g++ \
      gcc \
      git \
      golang-go \
      jq \
      make \
      nodejs \
      npm \
      openjdk-17-jdk-headless \
      openssh-client \
      pkg-config \
      python3 \
      python3-pip \
      python3-venv \
      ripgrep \
      rustc \
      unzip \
      wget \
      zip \
    && npm install -g pnpm yarn \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_REGISTRIES_CRATES_IO_INDEX=sparse+https://index.crates.io/
ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /opt/chatos-sandbox-build
COPY Cargo.lock ./sandbox_manager_service/sandbox_mcp_server/Cargo.lock
COPY crates ./crates
COPY sandbox_manager_service/sandbox_mcp_server ./sandbox_manager_service/sandbox_mcp_server
WORKDIR /opt/chatos-sandbox-build/sandbox_manager_service/sandbox_mcp_server
RUN cargo build --release --locked \
    && cp target/release/chatos_sandbox_mcp_server /usr/local/bin/chatos-sandbox-mcp-server \
    && cd / \
    && rm -rf /opt/chatos-sandbox-build

WORKDIR /
RUN mkdir -p /workspace /opt/chatos-sandbox-agent

WORKDIR /workspace
EXPOSE 49888

ENTRYPOINT ["chatos-sandbox-mcp-server"]
