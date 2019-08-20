from json import dumps as json_dumps

from api import KeyValueStoreApi, DELETE, POST, PUT, GET


class Action():
    path = '/kv'

    def __init__(self, method, expected_code, body, json=True, key=None, expected_body=None, comment=None):
        self.method = method
        self.ex_code = expected_code

        if json:
            if type(body) == tuple:
                self.body = KeyValueStoreApi.make_payload(body[0], body[1])
            else:
                self.body = json_dumps({'value': body})
        else:
            self.body = body
            
        self.key = key if key else None
        self.ex_body = expected_body
        self.cmt = comment

    def make_path(self):
        return '{}/{}'.format(self.path, self.key) if self.key else self.path 

    def perform(self, tester, api):
        code, _ = api.make_request(self.method, self.make_path(), data=self.body if self.body else None)
        with tester.subTest(
                comment=self.cmt if self.cmt else "without comments",
                case=str(self)):
            tester.assertEqual(code, self.ex_code)

    def __str__(self):
        return "{method}\n\tpayload: '{payload}'\n\texpected code: {ex_code}".format(
                method=self.method, payload=self.body, ex_code=self.ex_code
            )
