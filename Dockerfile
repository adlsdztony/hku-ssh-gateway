FROM ubuntu:22.04
ENV DEBIAN_FRONTEND=noninteractive
RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates openconnect vpnc-scripts iproute2 iptables netcat-openbsd procps tini \
    && rm -rf /var/lib/apt/lists/* \
    && useradd --uid 1100 --create-home --shell /usr/sbin/nologin jump
COPY container/ /usr/local/lib/hku/
RUN chmod 755 /usr/local/lib/hku/*.sh
ENTRYPOINT ["/usr/bin/tini", "-g", "--", "/usr/local/lib/hku/entrypoint.sh"]
