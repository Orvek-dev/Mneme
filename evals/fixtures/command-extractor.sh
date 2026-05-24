#!/usr/bin/env sh
set -eu

request=$(cat)

case "$request" in
  *"I work best with local-first tools; please remember that."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"local-first tools"}}'
    ;;
  *"When you summarize options for me, I prefer compact tables."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"compact tables for option summaries"}}'
    ;;
  *"For future planning docs, keep explanations direct and skip motivational language."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"direct explanations in future planning docs"}}'
    ;;
  *"Please don't use tables for tradeoff summaries; bullets are easier for me."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"bullets for tradeoff summaries"}}'
    ;;
  *"For Project Atlas, I want risk notes grouped by owner."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"prefers","object":"Project Atlas risk notes grouped by owner"}}'
    ;;
  *"Thanks, that answer helps."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"For this one task, use Python to rename the file."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"Sam prefers Vim; I do not."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"The test fixture says 'user prefers tabs', but that is only sample data."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"In this answer, do not mention Mneme."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
  *"My API_KEY=FAKE_TEST_VALUE should never become active memory."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"note","object":"API_KEY=FAKE_TEST_VALUE"}}'
    ;;
  *"The temporary token is TOKEN=FAKE_TOKEN_VALUE; do not expose it."*)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":{"subject":"user","predicate":"note","object":"TOKEN=FAKE_TOKEN_VALUE"}}'
    ;;
  *)
    printf '%s\n' '{"schema_version":"mneme.extractor.command.v1","claim":null}'
    ;;
esac
