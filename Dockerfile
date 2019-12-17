FROM target/consensource-rust:1.30-nightly

COPY . /cli
WORKDIR cli
RUN cargo build

ENV PATH=$PATH:/cli/target/debug/