# SPDX-License-Identifier: PolyForm-Noncommercial-1.0.0
# Required Notice: Copyright (c) 2025 AI Chat Team

FROM mcr.microsoft.com/devcontainers/base:dev-ubuntu-24.04 AS agent-builder

ENV DEBIAN_FRONTEND=noninteractive

RUN apt-get update \
    && apt-get install -y --no-install-recommends \
      ca-certificates \
      cargo \
      curl \
      g++ \
      gcc \
      git \
      make \
      pkg-config \
      rustc \
    && apt-get clean \
    && rm -rf /var/lib/apt/lists/*

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_REGISTRIES_CRATES_IO_INDEX=sparse+https://rsproxy.cn/index/
ENV CARGO_NET_RETRY=10
ENV CARGO_HTTP_TIMEOUT=600
ENV RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
ENV RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup

RUN mkdir -p /root/.cargo \
    && printf '%s\n' \
      '[source.crates-io]' \
      'replace-with = "rsproxy-sparse"' \
      '' \
      '[source.rsproxy-sparse]' \
      'registry = "sparse+https://rsproxy.cn/index/"' \
      '' \
      '[net]' \
      'git-fetch-with-cli = true' \
      > /root/.cargo/config.toml

RUN curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
      | sh -s -- -y --profile minimal --default-toolchain stable

ENV PATH=/root/.cargo/bin:$PATH

WORKDIR /opt/chatos-sandbox-build
COPY sandbox_manager_service/sandbox_agent/Cargo.workspace.toml ./Cargo.toml
COPY Cargo.lock ./Cargo.lock
COPY BUILTIN_MCP_PROMPT.en-US.md ./BUILTIN_MCP_PROMPT.en-US.md
COPY BUILTIN_MCP_PROMPT.zh-CN.md ./BUILTIN_MCP_PROMPT.zh-CN.md
COPY crates ./crates
COPY sandbox_manager_service/sandbox_mcp_server ./sandbox_manager_service/sandbox_mcp_server
RUN cargo build --release -p chatos_sandbox_mcp_server \
    && cp target/release/chatos_sandbox_mcp_server /usr/local/bin/chatos-sandbox-mcp-server

FROM mcr.microsoft.com/devcontainers/base:dev-ubuntu-24.04

ARG SANDBOX_FEATURES=java@21,node@24,rust@stable,go@1.26
ARG SANDBOX_CUSTOM_SCRIPT_B64=

ENV DEBIAN_FRONTEND=noninteractive
ENV CHATOS_AGENT_HOST=0.0.0.0
ENV CHATOS_AGENT_PORT=49888
ENV CHATOS_WORKSPACE=/workspace
ENV DOTNET_ROOT=/usr/local/dotnet
ENV JAVA_HOME=/usr/lib/jvm/default-jvm
ENV PIP_BREAK_SYSTEM_PACKAGES=1
ENV UV_BREAK_SYSTEM_PACKAGES=1
ENV PATH=/usr/local/go/bin:/usr/local/dotnet:/root/.cargo/bin:/root/.local/bin:$PATH
ENV RUSTUP_DIST_SERVER=https://mirrors.tuna.tsinghua.edu.cn/rustup
ENV RUSTUP_UPDATE_ROOT=https://mirrors.tuna.tsinghua.edu.cn/rustup/rustup

RUN set -eux; \
    java_version=""; \
    node_major=""; \
    python_version=""; \
    go_minor=""; \
    rust_channel=""; \
    dotnet_channel=""; \
    php_version=""; \
    ruby_version=""; \
    gcc_version=""; \
    clang_version=""; \
    for raw_token in $(printf '%s' "$SANDBOX_FEATURES" | tr ',' ' '); do \
      token="$(printf '%s' "$raw_token" | tr '[:upper:]' '[:lower:]')"; \
      runtime="${token%@*}"; \
      version="${token#*@}"; \
      if [ "$runtime" = "$version" ]; then version=""; fi; \
      case "$runtime" in \
        java|jdk|openjdk) java_version="${version:-21}" ;; \
        js|node|nodejs|javascript|typescript) node_major="${version:-24}" ;; \
        python|python3|py) python_version="${version:-3.14}" ;; \
        rust|cargo) rust_channel="${version:-stable}" ;; \
        go|golang) go_minor="${version:-1.26}" ;; \
        dotnet|csharp|cs|fsharp) dotnet_channel="${version:-10.0}" ;; \
        php) php_version="${version:-8.4}" ;; \
        ruby|rails) ruby_version="${version:-3.4.10}" ;; \
        c|cpp|c++|cplusplus|gcc|g++) gcc_version="${version:-14}" ;; \
        clang|llvm) clang_version="${version:-20}" ;; \
        "") true ;; \
        *) echo "unsupported sandbox runtime: $runtime" >&2; exit 2 ;; \
      esac; \
    done; \
    base_packages="bash ca-certificates cmake curl g++ gcc git gnupg jq make openssh-client pkg-config python3 python3-pip python3-venv ripgrep software-properties-common unzip wget xz-utils zip"; \
    apt-get update; \
    apt-get install -y --no-install-recommends $base_packages; \
    apt-get clean; \
    rm -rf /var/lib/apt/lists/*; \
    if [ -n "$java_version" ]; then \
      case "$java_version" in 8|11|17|21|25) true ;; *) echo "unsupported Java/JDK version: $java_version" >&2; exit 2 ;; esac; \
      mkdir -p /etc/apt/keyrings; \
      wget -qO - https://packages.adoptium.net/artifactory/api/gpg/key/public | tee /etc/apt/keyrings/adoptium.asc >/dev/null; \
      . /etc/os-release; \
      echo "deb [signed-by=/etc/apt/keyrings/adoptium.asc] https://packages.adoptium.net/artifactory/deb ${VERSION_CODENAME} main" > /etc/apt/sources.list.d/adoptium.list; \
      apt-get update; \
      apt-get install -y --no-install-recommends "temurin-${java_version}-jdk"; \
      ln -sfn "$(dirname "$(dirname "$(readlink -f "$(command -v javac)")")")" "$JAVA_HOME"; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$node_major" ]; then \
      case "$node_major" in 20|22|24|26) true ;; *) echo "unsupported Node.js version: $node_major" >&2; exit 2 ;; esac; \
      curl -fsSL "https://deb.nodesource.com/setup_${node_major}.x" | bash -; \
      apt-get install -y --no-install-recommends nodejs; \
      npm install -g pnpm yarn; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$python_version" ]; then \
      case "$python_version" in 3.10|3.11|3.12|3.13|3.14) true ;; *) echo "unsupported Python version: $python_version" >&2; exit 2 ;; esac; \
      curl -LsSf https://astral.sh/uv/install.sh | sh; \
      uv python install "$python_version"; \
      python_path="$(uv python find "$python_version")"; \
      python_bin_dir="$(dirname "$python_path")"; \
      ln -sf "$python_path" /usr/local/bin/python; \
      ln -sf "$python_path" /usr/local/bin/python3; \
      "$python_path" -m ensurepip --upgrade || true; \
      uv pip install --system --break-system-packages --python "$python_path" --upgrade pip setuptools wheel; \
      ln -sf "$python_bin_dir/pip" /usr/local/bin/pip || true; \
      ln -sf "$python_bin_dir/pip3" /usr/local/bin/pip3 || true; \
    fi; \
    if [ -n "$go_minor" ]; then \
      case "$go_minor" in \
        1.22) go_version="1.22.12" ;; \
        1.23) go_version="1.23.12" ;; \
        1.24) go_version="1.24.13" ;; \
        1.25) go_version="1.25.11" ;; \
        1.26) go_version="1.26.4" ;; \
        *) echo "unsupported Go version: $go_minor" >&2; exit 2 ;; \
      esac; \
      case "$(dpkg --print-architecture)" in \
        amd64) go_arch="amd64" ;; \
        arm64) go_arch="arm64" ;; \
        *) echo "unsupported Go architecture: $(dpkg --print-architecture)" >&2; exit 2 ;; \
      esac; \
      curl -fsSL "https://go.dev/dl/go${go_version}.linux-${go_arch}.tar.gz" -o /tmp/go.tgz; \
      rm -rf /usr/local/go; \
      tar -C /usr/local -xzf /tmp/go.tgz; \
      rm /tmp/go.tgz; \
    fi; \
    if [ -n "$rust_channel" ]; then \
      case "$rust_channel" in stable|beta|nightly|1.85.1|1.88.0|1.92.0|1.96.1) true ;; *) echo "unsupported Rust channel: $rust_channel" >&2; exit 2 ;; esac; \
      curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs \
        | sh -s -- -y --profile minimal --default-toolchain "$rust_channel"; \
    fi; \
    if [ -n "$dotnet_channel" ]; then \
      case "$dotnet_channel" in 8.0|9.0|10.0) true ;; *) echo "unsupported .NET version: $dotnet_channel" >&2; exit 2 ;; esac; \
      curl -fsSL https://dot.net/v1/dotnet-install.sh -o /tmp/dotnet-install.sh; \
      bash /tmp/dotnet-install.sh --channel "$dotnet_channel" --install-dir "$DOTNET_ROOT"; \
      ln -sf "$DOTNET_ROOT/dotnet" /usr/local/bin/dotnet; \
      rm /tmp/dotnet-install.sh; \
    fi; \
    if [ -n "$php_version" ]; then \
      case "$php_version" in 8.2|8.3|8.4|8.5) true ;; *) echo "unsupported PHP version: $php_version" >&2; exit 2 ;; esac; \
      LC_ALL=C.UTF-8 add-apt-repository -y ppa:ondrej/php; \
      apt-get update; \
      apt-get install -y --no-install-recommends \
        "php${php_version}" \
        "php${php_version}-cli" \
        "php${php_version}-common" \
        "php${php_version}-curl" \
        "php${php_version}-mbstring" \
        "php${php_version}-sqlite3" \
        "php${php_version}-xml" \
        "php${php_version}-zip"; \
      ln -sf "/usr/bin/php${php_version}" /usr/local/bin/php; \
      curl -fsSL https://getcomposer.org/installer -o /tmp/composer-setup.php; \
      php /tmp/composer-setup.php --install-dir=/usr/local/bin --filename=composer; \
      rm /tmp/composer-setup.php; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$ruby_version" ]; then \
      case "$ruby_version" in 3.2.11|3.3.11|3.4.10|4.0.5) true ;; *) echo "unsupported Ruby version: $ruby_version" >&2; exit 2 ;; esac; \
      apt-get update; \
      apt-get install -y --no-install-recommends autoconf bison libdb-dev libffi-dev libgdbm-dev libncurses-dev libreadline-dev libssl-dev libyaml-dev uuid-dev zlib1g-dev; \
      git clone --depth 1 https://github.com/rbenv/ruby-build.git /tmp/ruby-build; \
      PREFIX=/usr/local /tmp/ruby-build/install.sh; \
      ruby-build "$ruby_version" "/opt/rubies/ruby-${ruby_version}"; \
      ln -sf "/opt/rubies/ruby-${ruby_version}/bin/ruby" /usr/local/bin/ruby; \
      ln -sf "/opt/rubies/ruby-${ruby_version}/bin/gem" /usr/local/bin/gem; \
      "/opt/rubies/ruby-${ruby_version}/bin/gem" update --system --no-document || true; \
      "/opt/rubies/ruby-${ruby_version}/bin/gem" install bundler --no-document; \
      ln -sf "/opt/rubies/ruby-${ruby_version}/bin/bundle" /usr/local/bin/bundle; \
      ln -sf "/opt/rubies/ruby-${ruby_version}/bin/bundler" /usr/local/bin/bundler; \
      rm -rf /tmp/ruby-build; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$gcc_version" ]; then \
      case "$gcc_version" in 13|14) true ;; *) echo "unsupported GCC version: $gcc_version" >&2; exit 2 ;; esac; \
      apt-get update; \
      apt-get install -y --no-install-recommends "gcc-${gcc_version}" "g++-${gcc_version}"; \
      ln -sf "/usr/bin/gcc-${gcc_version}" /usr/local/bin/gcc; \
      ln -sf "/usr/bin/g++-${gcc_version}" /usr/local/bin/g++; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$clang_version" ]; then \
      case "$clang_version" in 18|19|20) true ;; *) echo "unsupported Clang version: $clang_version" >&2; exit 2 ;; esac; \
      apt-get update; \
      apt-get install -y --no-install-recommends "clang-${clang_version}" "clangd-${clang_version}" "lld-${clang_version}" "lldb-${clang_version}"; \
      ln -sf "/usr/bin/clang-${clang_version}" /usr/local/bin/clang; \
      ln -sf "/usr/bin/clang++-${clang_version}" /usr/local/bin/clang++; \
      ln -sf "/usr/bin/clangd-${clang_version}" /usr/local/bin/clangd; \
      apt-get clean; \
      rm -rf /var/lib/apt/lists/*; \
    fi; \
    if [ -n "$SANDBOX_CUSTOM_SCRIPT_B64" ]; then \
      set +x; \
      printf '%s' "$SANDBOX_CUSTOM_SCRIPT_B64" | base64 -d > /tmp/chatos-custom-build.sh; \
      chmod 700 /tmp/chatos-custom-build.sh; \
      echo "running custom sandbox image build script"; \
      /bin/bash /tmp/chatos-custom-build.sh; \
      rm -f /tmp/chatos-custom-build.sh; \
    fi

ENV CARGO_REGISTRIES_CRATES_IO_PROTOCOL=sparse
ENV CARGO_REGISTRIES_CRATES_IO_INDEX=sparse+https://rsproxy.cn/index/
ENV CARGO_NET_RETRY=10
ENV CARGO_HTTP_TIMEOUT=600

COPY --from=agent-builder /usr/local/bin/chatos-sandbox-mcp-server /usr/local/bin/chatos-sandbox-mcp-server

WORKDIR /
RUN mkdir -p /workspace /opt/chatos-sandbox-agent

WORKDIR /workspace
EXPOSE 49888

ENTRYPOINT ["chatos-sandbox-mcp-server"]
