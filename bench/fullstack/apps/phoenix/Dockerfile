# Modeled on `mix phx.gen.release --docker`: build a release, run it on a slim Debian.
FROM elixir:1.19-otp-28 AS build
WORKDIR /app
ENV MIX_ENV=prod
RUN mix local.hex --force && mix local.rebar --force
COPY mix.exs mix.lock ./
RUN mix deps.get --only prod
COPY config config
RUN mix deps.compile
COPY lib lib
COPY priv priv
COPY assets assets
RUN mix compile && mix esbuild.install --if-missing && mix assets.deploy && mix release

FROM debian:trixie-slim
RUN apt-get update -qq && apt-get install -y -qq --no-install-recommends libstdc++6 openssl libncurses6 locales ca-certificates >/dev/null && rm -rf /var/lib/apt/lists/* \
  && sed -i '/en_US.UTF-8/s/^# //g' /etc/locale.gen && locale-gen >/dev/null
ENV LANG=en_US.UTF-8 LC_ALL=en_US.UTF-8 MIX_ENV=prod PHX_SERVER=true PORT=8080
WORKDIR /app
COPY --from=build /app/_build/prod/rel/item_page ./
EXPOSE 8080
CMD ["/app/bin/item_page", "start"]
