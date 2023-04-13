Grafana Monitoring for relayer
===

Use the following steps for monitoring:

- Execute relayer on localhost as usual
- Alternatively, change the target value from `localhost:9955` to the address of a remote relayer
- Run `docker-compose up` in this directory
- Open Prometheus on `http://localhost:9090`. You can directly query for data
- Open Grafana on `http://localhost:3000`. Use the following steps for initial setup:
    - Login with `admin` / `admin`
    - Click the cogwheel icon on the bottom left and go to "Data sources"
    - Click "Add new data source", add Prometheus with URL `http://localhost:9090/` and click "Save & test"
    - Click "Dashboard" in the top left menu and select "Import"
    - Upload `grafana-dashboard.json` from this folder
