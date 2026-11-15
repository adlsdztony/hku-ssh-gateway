ARG ALPINE_IMAGE=alpine:3.23.7
FROM ${ALPINE_IMAGE}
RUN apk add --no-cache build-base linux-headers gnutls-dev libxml2-dev lz4-dev zlib-dev \
    python3 ca-certificates curl git abuild gnupg
WORKDIR /build
COPY third_party/openconnect-release-key.asc third_party/openconnect-9.21.tar.gz.asc /build/
RUN curl -fL --retry 3 https://casper.infradead.org/openconnect/download/openconnect-9.21.tar.gz -o openconnect.tar.gz \
    && echo '5b32369467db6e5f317aa1ed12cfcbb81ed00bdbc765450b6bfcbdc300944a58  openconnect.tar.gz' | sha256sum -c - \
    && mkdir -m 700 /build/release-keyring \
    && gpg --homedir /build/release-keyring --import /build/openconnect-release-key.asc \
    && gpg --homedir /build/release-keyring --verify /build/openconnect-9.21.tar.gz.asc openconnect.tar.gz \
    && tar -xzf openconnect.tar.gz
WORKDIR /build/openconnect-9.21
RUN ./configure --prefix=/usr --sbindir=/usr/bin --disable-static --disable-rpath --disable-nls \
    --without-openssl --with-gnutls --without-libproxy --without-gssapi \
    --without-libpskc --without-stoken --without-libpcsclite \
    --with-vpnc-script=/etc/vpnc/vpnc-script \
    && make -j2 && make DESTDIR=/build/stage install
COPY third_party/vpnc-scripts-ce9e961.tar.gz /build/vpnc-scripts.tar.gz
RUN echo '0b5906377890f41c7d07e3353d2f97c8d28ed1aaf25668d5f6dc80294f241a84  /build/vpnc-scripts.tar.gz' | sha256sum -c -
COPY scripts/runtime/collect.py /build/collect.py
COPY scripts/runtime/Dockerfile /build/build-recipe.Dockerfile
COPY scripts/build-runtime.sh /build/build-runtime.sh
COPY tests/mock-openconnect.c /build/mock-openconnect.c
RUN mkdir -p /out && cc -O2 -Wall -Wextra -Werror /build/mock-openconnect.c -o /out/mock-openconnect \
    && python3 /build/collect.py
