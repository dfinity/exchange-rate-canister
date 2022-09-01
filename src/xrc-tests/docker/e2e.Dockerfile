FROM e2e-base

ARG SCENARIO_DIRECTORY

# Install all of the generated files
ADD /src/xrc-tests/gen/${SCENARIO_DIRECTORY}/nginx/generate-certs-and-keys.sh /docker-entrypoint.d/
RUN chmod +x /docker-entrypoint.d/generate-certs-and-keys.sh
