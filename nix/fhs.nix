{
  buildFHSEnv,
  bubblewrap,
  shvia-desktop,
  nodejs,
  docker,
  docker-compose,
  openssl,
  glibc,
  uv,
}:
buildFHSEnv {
  name = "shvia-desktop";

  targetPkgs = pkgs: [
    bubblewrap
    shvia-desktop
    docker
    docker-compose
    glibc
    nodejs
    openssl
    uv
  ];

  runScript = "${shvia-desktop}/bin/shvia-desktop";

  extraInstallCommands = ''
    # Copy desktop file
    mkdir -p $out/share/applications
    cp ${shvia-desktop}/share/applications/* $out/share/applications/

    # Copy icons
    mkdir -p $out/share/icons
    cp -r ${shvia-desktop}/share/icons/* $out/share/icons/
  '';

  meta = shvia-desktop.meta // {
    description = "Claude Desktop for Linux (FHS environment for MCP servers)";
  };
}
