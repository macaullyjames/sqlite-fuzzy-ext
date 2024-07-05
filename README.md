# sqlite-fuzzy-ext
Very simple sqlite fuzzy extension writtin in rust. It will score most heavily on the end of a string.

You have to match the query with `like` to make the search correct, the extension contains only one function:

```sql
SELECT 
    * 
FROM 
    all_lines 
WHERE text like '%p%n%v%i%m%' ORDER BY fuzzy_score('pnvim', text)
LIMIT 300;
```

## Scoring
The score is most heavy on the end of a string, considering the search `pnvim` on the following entries:
```
Project/something/nvim
Project/nvim/lib/lua
```

The top entry will score higher.

Another bonus is given on the length of the text, this is because the long text is often not the result you want, 
and you can easily prefix some letters to the `like` query to make the long text the first result any way.

## Tips
Useful setting for fuzzy search:

```sql
PRAGMA case_sensitive_like=ON;
```
