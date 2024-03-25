from typing import Dict, List, Tuple
import graphviz
import tomllib as tl
import psycopg2 as psql

config = None

with open("config.dev.toml") as f:
    content = f.read()
    config = tl.loads(content)

pairs: List[Tuple[int, int]] = []
tokens: Dict[int, str] = {}

with psql.connect(config["database"]["url"]) as conn:
    # get the tokens ids as edges from DB
    with conn.cursor() as cur:
        cur.execute("SELECT token0, token1 FROM pairs")

        if (res := cur.fetchall()) is not None:
            for pair in res:
                (token0, token1) = pair

                pairs.append((token0, token1))

    # get the tokens from DB as dict of id -> token sybmol

    with conn.cursor() as cur:
        cur.execute("SELECT id, symbol FROM tokens")

        if (res := cur.fetchall()) is not None:
            for token in res:
                (id, symbol) = token

                tokens[id] = symbol

dot = graphviz.Graph(engine="sfdp")

for token0, token1 in pairs:
    symbol0 = tokens[token0]
    symbol1 = tokens[token1]

    dot.node(symbol0)
    dot.node(symbol1)
    dot.edge(symbol0, symbol1)

dot.render(
    "assets/graph",
    format="dot",
    cleanup=True,
)

# then call:
#
# sfdp -x -Goverlap=scale -Tpng ./assets/graph.dot > ./assets/data.png
#
