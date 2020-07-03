docker:
	docker build -f build/Dockerfile -t tarantool-operator .

crds:
	operator-sdk generate crds
	cp -r deploy/* ./ci/helm-chart/templates/
