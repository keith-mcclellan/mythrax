
## Vector search vs full-text search

SurrealDB supports [full-text search](\/docs\/learn\/data-models\/full-text-search\/overview) and Vector Search. Full-text search (FTS) involves indexing documents using an [FTS index](\/docs\/reference\/query-language\/statements\/define\/indexes#full-text-search-fulltext-index) that makes use of an [analyzer](\/docs\/reference\/query-language\/statements\/define\/analyzer) that breaks down text using [tokenizers](\/docs\/reference\/query-language\/statements\/define\/analyzer#tokenizers) and [filters](\/docs\/reference\/query-language\/statements\/define\/analyzer#filters).

The image above is a Google search for the word “lead”, a word with more than one definition (and pronunciation!). Lead can mean 'taking initiative', as well as the chemical element with the symbol 'Pb'.

Let's consider this in the context of a database of liquid samples which note down harmful chemicals that are found in them.

In the example below, we have a table called `liquids` with a `sample` field and a `content` field.  Next, we can define a [full-text index](\/docs\/reference\/query-language\/statements\/define\/indexes#full-text-search-index) on the `content` field by first defining an analyzer called `liquid_analyzer`. We can then [define an index](\/docs\/reference\/query-language\/statements\/define\/indexes) on the content field in the liquid table and set our [custom analyzer](\/docs\/reference\/query-language\/statements\/define\/analyzer) (`liquid_analyzer`)to search through the index.

Then, using the select statement to retrieve all the samples containing the chemical lead will also bring up samples that mention the word `lead`.

If you read through the content of the tap water sample, you’ll notice that it does not contain any lead in it but it has the mention of the word `lead` under “The team lead by Dr. Rose…” which means that the team was guided by Dr. Rose.

The search pulled up both the records although the tap water sample had no lead in it. This example shows us that while full-text search does a great job at matching query terms with indexed documents, on its own it may not be the best solution for use cases where the query terms have deeper context and scope for ambiguity.

For vector-side retrieval on the same story, see [Similarity search](\/docs\/learn\/data-models\/vector-search\/similarity-search).

## Hybrid search functions

As mentioned above, full-text search and vector search can both be used in SurrealDB. In addition, some functions exist inside the [`search::`](\/docs\/reference\/query-language\/functions\/database-functions\/search) namespace that take both full-text and vector arguments in order to produce a single unified output.

Here is an example of one of them called [`search::rrf()`](\/docs\/reference\/query-language\/functions\/database-functions\/search#searchrrf) which does this using an algorithm called reciprocal rank fusion.

```surql
-- Sample data --
CREATE test:1 SET text = "Graph databases are great.", embedding = [0.10, 0.20, 0.30];
CREATE test:2 SET text = "Relational databases store tables.", embedding = [0.05, 0.10, 0.00];
CREATE test:3 SET text = "This document mentions graphs.", embedding = [0.20, 0.10, 0.25];

-- Analyzer used by the full‑text index
DEFINE ANALYZER simple TOKENIZERS class, punct FILTERS lowercase, ascii;

-- Full‑text index
DEFINE INDEX idx_text
  ON TABLE test FIELDS text FULLTEXT ANALYZER simple BM25;
```

```surql
DEFINE INDEX idx_embedding
    ON TABLE test 
    FIELDS embedding 
    HNSW DIMENSION 3 DIST COSINE;
```

```surql
DEFINE INDEX idx_embedding
    ON TABLE test 
    FIELDS embedding 
    DISKANN DIMENSION 3 DIST COSINE TYPE F32;
```

For very large embedding sets that do not fit comfortably in RAM, prefer DISKANN. It is **not available on WASM** builds.

```surql
-- Query vector (whatever your embedding model produced for "graph databases")
LET $qvec = [0.12, 0.18, 0.27];

-- Vector search: top 2 nearest neighbours
LET $vs = SELECT id FROM test  WHERE embedding <|2,100|> $qvec;

-- Full‑text search: top 2 lexical matches
LET $ft = SELECT id, search::score(1) as score FROM test
          WHERE text @1@ 'graph' ORDER BY score DESC LIMIT 2;

-- Fuse with Reciprocal Rank Fusion (k defaults to 60 if omitted)
search::rrf([$vs, $ft], 2, 60);
```
