box.cfg({listen = 3301})

box.schema.func.create('hyper_client.get', {language = 'C', if_not_exists = true})
box.schema.user.grant('guest', 'execute', 'function', 'hyper_client.get', {if_not_exists = true})

require 'console'.start()
