{ python311 }:
python311.withPackages (ps:
  with ps; [
    # tools
    python
    pip
    setuptools
    wheel
    virtualenv
    venvShellHook
    black
    # libraries
    graphviz
    psycopg2
  ])
