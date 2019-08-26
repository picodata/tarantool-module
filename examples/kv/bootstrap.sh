#!/usr/bin/env bash

# You can do it manually in web ui at http://localhost:8081/
curl -X POST http://localhost:8081/admin/api -d@- <<'QUERY'
{"query":
    "mutation {
        j1: join_server(
            uri:\"topology:3301\",
            instance_uuid: \"aaaaaaaa-aaaa-4000-b000-000000000001\",
            replicaset_uuid: \"aaaaaaaa-0000-4000-b000-000000000000\",
            roles: [\"topology\"]
        )
        j2: join_server(
            uri:\"router:3301\",
            instance_uuid: \"bbbbbbbb-bbbb-4000-b000-000000000001\",
            replicaset_uuid: \"bbbbbbbb-0000-4000-b000-000000000000\",
            roles: [\"router\"],
            timeout: 5
        )
        j3: join_server(
            uri:\"storage:3301\",
            instance_uuid: \"cccccccc-cccc-4000-b000-000000000001\",
            replicaset_uuid: \"cccccccc-0000-4000-b000-000000000000\",
            roles: [\"storage\"],
            timeout: 5
        )

        bootstrap_vshard

        cluster {
            failover(enabled:true)
        }
    }"
}
QUERY
