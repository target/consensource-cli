FROM target/consensource-rust:cli-1.30

COPY . /cli
WORKDIR cli
RUN cargo build

ENV PATH=$PATH:/cli/target/debug/