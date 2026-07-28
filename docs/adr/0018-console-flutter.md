# Console is a Flutter multi-platform client

Console ships as one Flutter codebase for mobile and desktop. Chosen so mobile and desktop stay peer operator surfaces with a single Slack-style layout implementation, without a WebSocket rewrite of the Worker or a dual RN/Electron packaging story. The app remains a thin Principal client of the HTTP/SSE control plane; it does not host the agent loop or share runtime with the Rust Worker beyond the API contract.
