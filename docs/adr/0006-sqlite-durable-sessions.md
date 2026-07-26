# Local SQLite for durable Sessions; Active Runs are not mid-loop resumed

Session transcripts and Run records persist in a local SQLite store on the Worker. An Active Run that dies with the process is marked failed/interrupted; clients continue by starting a new Run on the same Session. Chosen over in-memory-only (too fragile for Mac/phone clients), full checkpoint resume (unsafe with tool side effects), and external databases (unnecessary ops for a single-operator worker).
