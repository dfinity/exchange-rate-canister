FROM e2e-base

ARG SCENARIO_DIRECTORY

# Install all of the generated files
ADD /src/xrc-tests/gen/${SCENARIO_DIRECTORY}/nginx/init.sh /docker-entrypoint.d/
RUN chmod +x /docker-entrypoint.d/init.sh
