# TODO: when app starts for migrations set env to disable connectivity checks and telemetry
# defmodule Domain.Telemetry do
#   @moduledoc """
#   Functions for various telemetry events.
#   """
#   use Supervisor
#   alias Domain.Telemetry.{Timer, PostHog}
#   require Logger

#   def start_link(opts) do
#     Supervisor.start_link(__MODULE__, opts, name: __MODULE__)
#   end

#   def init(_opts) do
#     config = Domain.Config.fetch_env!(:domain, Domain.Telemetry)

#     if Keyword.fetch!(config, :enabled) == true do
#       children = [Timer]
#       Supervisor.init(children, strategy: :one_for_one)
#     else
#       :ignore
#     end
#   end

#   def create_api_token do
#     PostHog.capture("add_api_token", common_fields())
#     :ok
#   end

#   def delete_api_token(api_token) do
#     PostHog.capture(
#       "delete_api_token",
#       common_fields() ++
#         [
#           api_token_created_at: api_token.inserted_at
#         ]
#     )

#     :ok
#   end

#   def add_device do
#     PostHog.capture("add_device", common_fields())
#     :ok
#   end

#   def add_actor do
#     PostHog.capture("add_actor", common_fields())
#     :ok
#   end

#   def add_rule do
#     PostHog.capture("add_rule", common_fields())
#     :ok
#   end

#   def delete_device do
#     PostHog.capture("delete_device", common_fields())
#     :ok
#   end

#   def delete_actor do
#     PostHog.capture("delete_actor", common_fields())
#     :ok
#   end

#   def delete_rule do
#     PostHog.capture("delete_rule", common_fields())
#     :ok
#   end

#   def login do
#     PostHog.capture("login", common_fields())
#     :ok
#   end

#   def enable_actor do
#     PostHog.capture("enable_actor", common_fields())
#     :ok
#   end

#   def disable_actor do
#     PostHog.capture("disable_actor", common_fields())
#     :ok
#   end

#   def domain_started do
#     PostHog.capture("domain_started", common_fields())
#     :ok
#   end

#   def ping do
#     PostHog.capture("ping", ping_data())
#     :ok
#   end

#   # How far back to count handshakes as an active device
#   # @active_device_window 86_400
#   def ping_data do
#     %{
#       local_auth_enabled: {_, local_auth_enabled},
#       logo: {_, logo}
#     } =
#       Domain.Config.fetch_resolved_configs_with_sources!([
#         :local_auth_enabled,
#         :logo
#       ])

#     common_fields() ++
#       [
#         # devices_active_within_24h: Devices.count_active_within(@active_device_window),
#         # admin_count: Users.count_by_role(:account_admin_user),
#         # actor_count: Users.count(),
#         in_docker: in_docker?(),
#         # device_count: Devices.count(),
#         # max_devices_for_actors: Devices.count_maximum_for_a_actor(),
#         # actors_with_mfa: MFA.count_actors_with_mfa_enabled(),
#         # actors_with_mfa_totp: MFA.count_actors_with_totp_method(),
#         local_authentication: local_auth_enabled,
#         # outbound_email: Web.Mailer.active?(),
#         external_database:
#           external_database?(Map.new(Domain.Config.fetch_env!(:domain, Domain.Repo))),
#         logo_type: Domain.Config.Logo.type(logo)
#       ]
#   end

#   defp in_docker? do
#     File.exists?("/.dockerenv")
#   end

#   defp common_fields do
#     [
#       distinct_id: id(),
#       fqdn: fqdn(),
#       version: version(),
#       kernel_version: "#{os_type()} #{os_version()}"
#     ]
#   end

#   def id do
#     Domain.Config.fetch_env!(:domain, __MODULE__)
#     |> Keyword.fetch!(:id)
#   end

#   defp fqdn do
#     :web
#     |> Domain.Config.fetch_env!(Web.Endpoint)
#     |> Keyword.get(:url)
#     |> Keyword.get(:host)
#   end

#   defp version do
#     Application.spec(:domain, :vsn) |> to_string()
#   end

#   defp external_database?(repo_conf) when is_map_key(repo_conf, :hostname) do
#     is_external_db?(repo_conf.hostname)
#   end

#   defp external_database?(repo_conf) when is_map_key(repo_conf, :url) do
#     %{host: host} = URI.parse(repo_conf.url)

#     is_external_db?(host)
#   end

#   defp is_external_db?(host) do
#     host != "localhost" && host != "127.0.0.1"
#   end

#   defp os_type do
#     case :os.type() do
#       {:unix, type} ->
#         "#{type}"

#       _ ->
#         "other"
#     end
#   end

#   defp os_version do
#     case :os.version() do
#       {major, minor, patch} ->
#         "#{major}.#{minor}.#{patch}"

#       _ ->
#         "0.0.0"
#     end
#   end
# end
