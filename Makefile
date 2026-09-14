SHELL := /bin/bash
.PHONY: update start stop restart status logs doctor test-release help

help:
	@printf 'STEALTHNET\n  make update       обновить установленную панель\n  make update VERSION=v0.2.6  выбрать опубликованную версию\n  make start        запустить службы\n  make stop         остановить службы\n  make restart      перезапустить службы\n  make status       состояние служб\n  make doctor       проверить релиз, API и базу\n  make logs         журнал API\n'

update:
	@bash ./update.sh $(if $(VERSION),--version '$(VERSION)',)

SERVICE_MANAGER := $(if $(wildcard current/web/service-manager.py),current/web/service-manager.py,web/service-manager.py)

start stop restart status:
	@python3 $(SERVICE_MANAGER) $@

logs:
	@stealthnet logs

doctor:
	@stealthnet doctor

test-release:
	@python3 devtools/release_version.py --check
	@python3 -m unittest discover -s devtools/tests -p 'test_*.py' -v
	@node --test devtools/tests/profile-form.test.cjs
	@python3 -m unittest discover -s deploy/tests -v
	@bash -n install.sh update.sh deploy/migrate.sh
