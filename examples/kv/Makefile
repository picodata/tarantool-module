url:=localhost:8082

# docker-compose commands
build:
	docker-compose build

start:
	docker-compose up -d

stop:
	docker-compose down

role:
	python3 tests/graphql_helper.py ${url}

test:
	python3 tests/tests.py ${url}
