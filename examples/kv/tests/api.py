#!/usr/bin/python3

import requests
import json


DELETE = 'DELETE'
POST = 'POST'
PUT = 'PUT'
GET = 'GET'


class KeyValueStoreApi():
    host = '127.0.0.1'
    port = 5000
    
    def __init__(self, host, port):
        self.host = host
        self.port = port
        self.methods = {
            GET: requests.get,
            PUT: requests.put,
            POST: requests.post,
            DELETE: requests.delete
        }
    
    def make_url(self, path):
        return 'http://{domain}:{port}{path}'.format(
            domain=self.host,
            port=self.port,
            path=path if path[0] == '/' else path[1:]
        )

    def make_request(self, method, path, **kwargs):
        r = self.methods[method](self.make_url(path), **kwargs)
        if r.status_code in (500, ):
            return r.status_code, None
        return r.status_code, r.content

    @staticmethod
    def make_payload(key, value):
        return json.dumps({'key': key, 'value': value})

