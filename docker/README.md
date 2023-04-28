Grafana Monitoring for relayer
===

Use the following steps for monitoring:

- Execute relayer on localhost as usual
- Alternatively, change the `targets` value in `prometheus.yml` from `localhost:9955` to the address of a remote relayer
- To enable alerts, you need to put the "Slack Incoming Webhook URL" in `provisioning/alerting/alerting.yaml` under `contactPoints` (optional)
- Run `docker-compose up` in this directory
- Open Grafana on `http://localhost:3000` and login with `admin` / `admin`
    - Dashboards and alerts are [automatically provisioned](https://grafana.com/docs/grafana/latest/administration/provisioning/)
