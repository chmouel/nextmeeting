PACKAGE_NAME := nextmeeting

all: lint run

run:
	@uv run $(PACKAGE_NAME)

release:
	@./packaging/make-release.sh

lint:
	@uv run ruff check src/$(PACKAGE_NAME)


.PHONY: lint release run all
