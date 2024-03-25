-- Table of the factory contracts deployed on the chains
CREATE TABLE IF NOT EXISTS factories (
    id SERIAL PRIMARY KEY,
    address CHAR(42) NOT NULL UNIQUE
);

-- Table of ERC20 tokens deployed on the chains
CREATE TABLE IF NOT EXISTS tokens (
    id SERIAL PRIMARY KEY,

    address CHAR(42) NOT NULL UNIQUE,

    name TEXT NOT NULL,
    symbol TEXT NOT NULL,
    decimals INTEGER NOT NULL
);

-- Table of the blocks that have been processed
CREATE TABLE IF NOT EXISTS blocks (
    id     SERIAL   PRIMARY KEY,

    height BIGINT   NOT NULL,
    hash   CHAR(66) NOT NULL UNIQUE
);

-- Table of the Uniswap pairs
CREATE TABLE IF NOT EXISTS pairs (
    id        SERIAL     PRIMARY KEY,
    factory   INTEGER    NOT NULL,
    address   CHAR(42)   NOT NULL UNIQUE,
    token0    INTEGER    NOT NULL,
    token1    INTEGER    NOT NULL,

    -- number of the pair in the factory
    number    INTEGER    NOT NULL,

    FOREIGN KEY (factory) REFERENCES factories(id),
    FOREIGN KEY (token0)  REFERENCES tokens(id),
    FOREIGN KEY (token1)  REFERENCES tokens(id)
);

-- Table of the reserves of the pairs got on particular blocks
CREATE TABLE IF NOT EXISTS reserves (
    id SERIAL PRIMARY KEY,
    pair INTEGER NOT NULL,
    block INTEGER NOT NULL,

    reserve0 NUMERIC NOT NULL,
    reserve1 NUMERIC NOT NULL,

    FOREIGN KEY (pair) REFERENCES pairs(id),
    FOREIGN KEY (block) REFERENCES blocks(id)
);
