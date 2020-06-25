FROM target/consensource-rust:cli-stable

COPY . /cli
WORKDIR cli
RUN cargo build

ENV PATH=$PATH:/cli/target/debug/
