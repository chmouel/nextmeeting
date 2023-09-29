PACKAGE_NAME := nextmeeting

all: lint run

run:
	@poetry run $(PACKAGE_NAME)

release:
	@./packaging/make-release.sh

lint:
	@poetry run ruff $(PACKAGE_NAME)


.PHONY: lint release run all
