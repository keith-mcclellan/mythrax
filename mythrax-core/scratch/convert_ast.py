import json

def ast_to_md(node):
    if not node:
        return ""
    ntype = node.get("type")
    if ntype == "root" or ntype == "blockquote" or ntype == "listItem":
        return "".join(ast_to_md(c) for c in node.get("children", []))
    elif ntype == "heading":
        depth = node.get("depth", 1)
        content = "".join(ast_to_md(c) for c in node.get("children", []))
        return "\n" + "#" * depth + " " + content + "\n"
    elif ntype == "paragraph":
        content = "".join(ast_to_md(c) for c in node.get("children", []))
        return "\n" + content + "\n"
    elif ntype == "list":
        return "\n" + "".join("- " + ast_to_md(c) for c in node.get("children", []))
    elif ntype == "text":
        return node.get("value", "")
    elif ntype == "inlineCode":
        return "`" + node.get("value", "") + "`"
    elif ntype == "code":
        return "\n```" + node.get("lang", "") + "\n" + node.get("value", "") + "\n```\n"
    elif ntype == "link":
        content = "".join(ast_to_md(c) for c in node.get("children", []))
        return f"[{content}]({node.get('url', '')})"
    elif ntype == "strong":
        content = "".join(ast_to_md(c) for c in node.get("children", []))
        return f"**{content}**"
    elif ntype == "emphasis":
        content = "".join(ast_to_md(c) for c in node.get("children", []))
        return f"*{content}*"
    else:
        children = node.get("children", [])
        if children:
            return "".join(ast_to_md(c) for c in children)
        return node.get("value", "")

def process_file(json_path, md_path):
    with open(json_path) as f:
        data = json.load(f)
    ast = data.get("data", {}).get("ast", {})
    md = ast_to_md(ast)
    with open(md_path, "w") as f:
        f.write(md)

process_file("/Users/keith/Documents/mythrax/mythrax-core/scratch/parsed_docs.json", "/Users/keith/Documents/mythrax/mythrax-core/scratch/scoring_and_ranking.md")
process_file("/Users/keith/Documents/mythrax/mythrax-core/scratch/hybrid_search_docs.json", "/Users/keith/Documents/mythrax/mythrax-core/scratch/hybrid_search.md")
