# -------------=== cli rust build ===-------------
FROM ubuntu:bionic as cli-rust-builder

ENV VERSION=0.1.0

RUN apt-get update \
 && apt-get install -y \
 curl \
 gcc \
 git \
 libssl-dev \
 libzmq3-dev \
 pkg-config \
 protobuf-compiler \
 unzip

# For Building Protobufs
RUN curl https://sh.rustup.rs -sSf | sh -s -- -y \
 && curl -OLsS https://github.com/google/protobuf/releases/download/v3.5.1/protoc-3.5.1-linux-x86_64.zip \
 && unzip protoc-3.5.1-linux-x86_64.zip -d protoc3 \
 && rm protoc-3.5.1-linux-x86_64.zip

ENV PATH=$PATH:/protoc3/bin
RUN /root/.cargo/bin/cargo install cargo-deb

COPY . /project

WORKDIR /project/

RUN /root/.cargo/bin/cargo deb --deb-version $VERSION

# -------------=== cli docker build ===-------------
FROM ubuntu:bionic

RUN apt-get update \
 && apt-get install gnupg -y

COPY --from=cli-rust-builder /project/target/debian/consensource-cli*.deb /tmp

RUN apt-get update \
 && dpkg -i /tmp/consensource-cli*.deb || true \
 && apt-get -f -y install \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*

# -------------=== install sawtooth binary ===-------------
RUN echo "deb [arch=amd64] http://repo.sawtooth.me/ubuntu/chime/stable bionic universe" >> /etc/apt/sources.list \
 && (apt-key adv --keyserver hkp://keyserver.ubuntu.com:80 --recv-keys 8AA7AF1F1091A5FD \
 || apt-key adv --keyserver hkp://p80.pool.sks-keyservers.net:80 --recv-keys 8AA7AF1F1091A5FD) \
 && apt-get update \
 && apt-get install -y software-properties-common \
 && add-apt-repository 'deb [arch=amd64] http://repo.sawtooth.me/ubuntu/chime/stable bionic universe' \
 && apt update \
 && apt install -y sawtooth \
 && apt-get clean \
 && rm -rf /var/lib/apt/lists/*