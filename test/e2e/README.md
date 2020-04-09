# Tarantool Operator end-2-end tests

Testing strategy:

1. provision kubernetes resources from YAML manifest

1. act on provisioned resources

1. assert expected system state

1. clean up 

Some conventions:

1. setup manifests located under `scenario` dir

1. tests must be independent and no reliance on previous tests allowed

1. tests must be able to run in parallel each test in it's own namespace

1. each setup manifest must be written in a way that allows to use it without tests