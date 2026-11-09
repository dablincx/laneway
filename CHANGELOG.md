# Changelog

All notable changes to laneway. Generated from the commit history, see CONTRIBUTING.md.

## 0.1.0 (2026-11-09)

### Features

#### auth

- argon2id passwords and hashed random tokens

#### config

- typed settings declared once, with env, _FILE and db layers
- database pool size and worker concurrency settings
- env-only bootstrap settings and per-setting value validation
- LANEWAY_PUBLIC_URL is env-only

#### core

- first-launch setup, login and sessions service
- aes-256-gcm encryption for secrets at rest
- database settings layer with encrypted secrets
- fractional position keys with property tests
- board, column and card service that records every change as an action

#### db

- postgres 18 pool, version check and embedded migrations
- users, sessions and setup token tables and queries
- load, upsert and delete rows of the settings table
- workspaces, boards, columns, cards and the action log

#### examples

- app + worker and all-in-one compose files, tested in ci

#### i18n

- fluent loader, fallback, negotiation and the translation check
- flag literal text in placeholder, title, aria-label and alt attributes

#### jobs

- postgres job queue with serial and replace keys, retries and leases

#### server

- laneway binary with app and worker roles, health checks and log files
- connect and migrate on start, readyz checks the database, worker runs the queue
- laneway i18n check command
- setup, login, logout and me endpoints with openapi spec and docs page
- load database settings on start, laneway settings list/set/unset
- merge the database settings layer, create the first admin from LANEWAY_INITIAL_ADMIN_* env vars
- worker-only health check tests the database
- boards, columns, cards and history endpoints
- worker deletes expired sessions every hour

#### web

- web ui skeleton with fluent i18n and english fallback
- setup and login pages on the generated api client
- board list and board view with columns and cards

### Refactoring

#### server

- look each setting up once in laneway settings list

### Documentation

#### config

- generated env var reference and .env.example

#### general

- readme, contributing, security, claude rules and example compose
- database settings and the laneway settings commands, tick the settings layer in TODO.md

### Build

#### docker

- production image, dev container and compose files

