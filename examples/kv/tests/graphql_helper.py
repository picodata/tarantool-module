#!/usr/bin/env python3

import sys
import json
import random
import termcolor
import requests
from graphqlclient import GraphQLClient
from collections import namedtuple
from urllib.error import URLError


__server_description_fields = [
    'uri', 'alias', 'status',
    'uuid', 'message', 'replicaset'
]

ServerDescription = namedtuple(
        'ServerDescription',
        __server_description_fields
    )


def find(predicate, seq, default=None):
    for s in seq:
        if predicate(s):
            return s
    return default


def parse_server_description(kwdict):
    return ServerDescription(*[
            (kwdict[prop] if prop in kwdict else None)
            for prop in __server_description_fields
        ])


def get_servers(url):
    client = GraphQLClient('http://{}/admin/api'.format(url))

    result = client.execute('''
        query {
            serverList: servers {
                uuid
                alias
                uri
                status
                message
                replicaset {
                    uuid
                }
            }
        }
    ''')

    data = json.loads(result)
    servers = list(map(
        parse_server_description,
        data['data']['serverList']
    ))

    return servers


def get_cluster_info(url):
    client = GraphQLClient('http://{}/admin/api'.format(url))

    result = client.execute('''
        query {
            cluster {
                clusterSelf: self {
                    uri: uri
                    uuid: uuid
                }
                failover
                knownRoles: known_roles
                can_bootstrap_vshard
                vshard_bucket_count
            }
        }
    ''')

    data = json.loads(result)
    return data['data']['cluster']


def assign_roles(url, server_description, roles):
    assert roles

    client = GraphQLClient('http://{}/admin/api'.format(url))

    result = client.execute('''
            mutation( $uri: String!, $roles: [String!] ) {
                createReplicasetResponse: join_server(
                    uri: $uri
                    roles: $roles
                )
            }
        ''',
        variables={
                **server_description._asdict(),
                'roles': roles
            }
        )

    data = json.loads(result)
    print(data)

    return (
        'data' in data
            and 'createReplicasetResponse' in data['data']
            and 'errors' not in data,
        data
    )


if __name__ == "__main__":

    url = '127.0.0.1:8081'

    ping = requests.get('http://{}/'.format(url), timeout=10)
    assert ping.status_code == 200

    servers = get_servers(url)

    # TODO prettify this output
    print('\n'.join(map(str, servers)))
    print('-' * 72)

    assert servers

    cluster_info = get_cluster_info(url)
    bootstraped_uri = cluster_info['clusterSelf']['uri']

    key_value_server = find(
        lambda server: server.uri == bootstraped_uri,
        servers,
        servers[0]
    )

    print(key_value_server)
    assert key_value_server
    assert 'key-value' in cluster_info['knownRoles']

    success, responce = assign_roles(url, key_value_server, ['key-value'])
    if success:
        print(termcolor.colored('success', 'green'))
    else:
        print(termcolor.colored('fail', 'red'))
        print(responce)
        exit(1)
