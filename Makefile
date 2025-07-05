PACKAGE_NAME := nextmeeting

all: lint run

sync:
	@uv sync

run: sync
	echo "Running $(PACKAGE_NAME)"
	@uv run $(PACKAGE_NAME)

release: sync
	echo "Creating Release"
	echo "----------------"
	@./packaging/make-release.sh

lint: sync
	echo "Running Linters"
	echo "-------------"
	@uv run ruff check src/$(PACKAGE_NAME)
	@uv run pylint src/$(PACKAGE_NAME)

test: sync
	@echo "Running Tests"
	@echo "-------------"
	@uv run pytest -sv tests

format: sync
	@echo "Running formatter"
	@echo "----------------"
	@uvx ruff format

coverage: sync
	@echo "Running coverage"
	@echo "---------------"
	@uv run pytest --cov=src/$(PROJECT_NAME) --cov-report=html --cov-report=term-missing ./tests

.PHONY: all run release lint test format coverage sync
