#!/usr/bin/python3
# -*- coding: utf-8 -*-

import sys
import unittest

from action import Action
from api import KeyValueStoreApi, DELETE, POST, PUT, GET


SEQUENCE = [
    Action(POST, 201, ('key_str', "Simple string on key1")),
    Action(POST, 201, ('key_arr', [1, 2, 3, 5, 8, 13, 21])),

    Action(POST, 400, '{"key": "key", value: 42}', json=False, comment='create: invalid json'),
    Action(POST, 400, '{"key": "key"}', json=False, comment='create: missing value'),
    Action(POST, 400, '{"value": 42}', json=False, comment='create: missing key'),
    Action(POST, 400, '\{\}', json=False, comment='create: empty json'),

    Action(GET, 200, key='key_str', body=None),
    Action(GET, 200, key='key_arr', body=None),
    Action(GET, 404, key='420', body=None, comment='get: key not found'),

    Action(DELETE, 404, key='*331', body=None, comment='delete: key not found'),

    Action(PUT, 200, [0, 0, 1, 1, 0, -42], key='key_arr'),
    Action(PUT, 400, "[0, 0, 1, 1, 0, -42]", json=False, key='key_arr', comment='update: invalid json'),
    Action(PUT, 400, '\{\}', key='key_arr',  json=False, comment='update: missing value'),
    Action(PUT, 404, [0, 0, 1, 1, 0, -42], key=' ', comment='empty key'),

    Action(DELETE, 200, key='key_str', body=None),
    Action(GET, 404, key='key_str', body=None, comment='delete: key "key_str" deleted'),
    Action(PUT, 404, "New string never updated", key='key_str', comment='update: key not found'),

    Action(POST, 201, ('key_obj', {'value_type': "string", 'secret': False, 'value': 'qwerty'})),
    Action(POST, 409, ('key_obj', {'value_type': "int", 'secret': True, 'value': -7}), comment='create: conflict keys'),
    Action(GET, 200, body=None, key='key_obj'),
    Action(DELETE, 200, key='key_obj', body=None),
    Action(GET, 404, key='key_obj', body=None, comment='delete: key "key_obj" deleted'),
]


class TestApi(unittest.TestCase):

    api = None

    def test_sequence(self, seq=SEQUENCE):
        for action in seq:
            action.perform(self, self.api)






if __name__ == '__main__':

    url = '127.0.0.1:80' if len(sys.argv) < 2 else sys.argv[1]
    host, port = url.split(":")
    port = int(port)

    if url in sys.argv:
        sys.argv.remove(url)

    TestApi.api = KeyValueStoreApi(host, port)
    unittest.main()
