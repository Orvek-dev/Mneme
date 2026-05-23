#!/usr/bin/env sh
set -eu

request=$(cat)

case "$request" in
  *"I work best with local-first tools; please remember that."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"local-first tools"}}'
    ;;
  *"Thanks, that answer helps."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"My API_KEY=FAKE_TEST_VALUE should never become active memory."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"note","object":"API_KEY=FAKE_TEST_VALUE"}}'
    ;;
  *)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
esac
