[Unit]
Description=Chatos Backend API Service
After=network.target
Wants=network-online.target

[Service]
Type=simple
User=__SERVICE_USER__
Group=__SERVICE_GROUP__
WorkingDirectory=__BACKEND_WORKDIR__
EnvironmentFile=__ENV_FILE__
ExecStart=__BACKEND_BIN__
Restart=always
RestartSec=3
KillMode=mixed
TimeoutStopSec=20
LimitNOFILE=65535
StandardOutput=journal
StandardError=journal

[Install]
WantedBy=multi-user.target
