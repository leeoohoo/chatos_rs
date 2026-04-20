SHELL := /bin/bash
RUNNER := tests/run_by_case.sh

.PHONY: help test test-all test-stream test-http test-request test-create-response test-response test-base test-core test-integration
.PHONY: test-mcp-request test-mcp-stream test-mcp-full test-function-single test-function-multi test-function-stream

help:
	@echo "openai-codex-gateway test targets:"
	@echo "  make test                 # same as test-all"
	@echo "  make test-all             # gateway unit regression (test_gateway_*.py)"
	@echo "  make test-stream          # tests/case_stream"
	@echo "  make test-http            # tests/case_http"
	@echo "  make test-request         # tests/case_request"
	@echo "  make test-create-response # tests/case_create_response"
	@echo "  make test-response        # tests/case_response"
	@echo "  make test-base            # tests/case_base"
	@echo "  make test-core            # tests/case_core"
	@echo "  make test-integration     # tests/case_integration"
	@echo "  make test-mcp-request     # integration script"
	@echo "  make test-mcp-stream      # integration script"
	@echo "  make test-mcp-full        # integration script"
	@echo "  make test-function-single # integration script"
	@echo "  make test-function-multi  # integration script"
	@echo "  make test-function-stream # integration script"

test: test-all

test-all:
	@bash $(RUNNER) all

test-stream:
	@bash $(RUNNER) stream

test-http:
	@bash $(RUNNER) http

test-request:
	@bash $(RUNNER) request

test-create-response:
	@bash $(RUNNER) create-response

test-response:
	@bash $(RUNNER) response

test-base:
	@bash $(RUNNER) base

test-core:
	@bash $(RUNNER) core

test-integration:
	@bash $(RUNNER) integration

test-mcp-request:
	@bash $(RUNNER) mcp-request

test-mcp-stream:
	@bash $(RUNNER) mcp-stream

test-mcp-full:
	@bash $(RUNNER) mcp-full

test-function-single:
	@bash $(RUNNER) function-single

test-function-multi:
	@bash $(RUNNER) function-multi

test-function-stream:
	@bash $(RUNNER) function-stream
