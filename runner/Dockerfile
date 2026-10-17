FROM ubuntu:24.04

# Prevent interactive prompts
ENV DEBIAN_FRONTEND=noninteractive

# Install core build dependencies for AnyPS5 C++ compilation
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates \
    curl \
    git \
    jq \
    sudo \
    tar \
    unzip \
    build-essential \
    cmake \
    ninja-build \
    clang-18 \
    llvm-18 \
    libvulkan-dev \
    vulkan-tools \
    libx11-dev \
    libxrandr-dev \
    libxinerama-dev \
    libxcursor-dev \
    libxi-dev \
    ccache \
    && rm -rf /var/lib/apt/lists/*

# Setup non-root runner user
RUN useradd -m -s /bin/bash runner \
    && echo "runner ALL=(ALL) NOPASSWD:ALL" >> /etc/sudoers

WORKDIR /home/runner

# Download official GitHub Actions Runner for Linux x64
ARG RUNNER_VERSION="2.322.0"
RUN curl -o actions-runner-linux-x64.tar.gz -L \
    https://github.com/actions/runner/releases/download/v${RUNNER_VERSION}/actions-runner-linux-x64-${RUNNER_VERSION}.tar.gz \
    && tar xzf ./actions-runner-linux-x64.tar.gz \
    && rm actions-runner-linux-x64.tar.gz \
    && ./bin/installdependencies.sh

COPY entrypoint.sh /home/runner/entrypoint.sh
RUN chmod +x /home/runner/entrypoint.sh && chown -R runner:runner /home/runner

USER runner

ENTRYPOINT ["/home/runner/entrypoint.sh"]
