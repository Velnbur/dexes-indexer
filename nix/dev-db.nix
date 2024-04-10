{ dbType ? "postgres" }:
let
  params = {
    user = "dex";
    password = "admin123";
    host = "127.0.0.1";
    port = 5432;
    database = "dex";
  };

  toURL = ({ user, password, host, port, database }:
    let portStr = toString port;
    in "${dbType}://${user}:${password}@${host}:${portStr}/${database}");

  toPsqlCmd = ({ user, database, ... }: "psql -U ${user} -d ${database}");

  toNixOsSettings = { user, password, host, port, database }:
    package: {
      inherit port package;

      listen_addresses = host;
      initialDatabases = [{ name = database; }];

      superuser = user;

      initialScripts = {
        before = ''
          CREATE USER ${user} WITH PASSWORD '${password}';
          CREATE DATABASE ${database} OWNER ${user};
        '';
      };
    };
in rec {
  # params for general usage
  inherit (params) user password host port database;

  # URL for connecting to the database
  url = toURL params;

  # Command for connecting to the database
  psqlCmd = toPsqlCmd params;

  # NixOS settings for the database
  nixos = toNixOsSettings params;
}
