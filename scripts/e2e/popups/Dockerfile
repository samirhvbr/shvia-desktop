FROM debian:trixie-slim

# Runtime and UI test tools only: no compiler, private image, signing key or account.
RUN apt-get update -qq \
    && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends \
      bash ca-certificates coreutils curl dbus-x11 findutils fonts-dejavu-core \
      imagemagick libayatana-appindicator3-1 libegl-mesa0 libgl1-mesa-dri \
      libwebkit2gtk-4.1-0 nodejs openbox openssl python3 wmctrl xauth xdotool xvfb \
    && useradd --uid 1000 --create-home --shell /bin/bash builder \
    && rm -rf /var/lib/apt/lists/*
