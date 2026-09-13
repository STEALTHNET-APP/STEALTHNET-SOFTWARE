SHELL := /bin/bash
.PHONY: update status logs doctor test-release help

help:
	@printf 'STEALTHNET\n  make update       обновить установленную панель\n  make update VERSION=v0.1.0  выбрать опубликованную версию\n  make status       состояние служб\n  make doctor       проверить релиз, API и базу\n  make logs         журнал API\n'

update:
	@bash ./update.sh $(if $(VERSION),--version '$(VERSION)',)

status:
	@stealthnet status

logs:
	@stealthnet logs

doctor:
	@stealthnet doctor

test-release:
	@python3 -m unittest discover -s deploy/tests -v
	@bash -n install.sh update.sh deploy/migrate.sh
