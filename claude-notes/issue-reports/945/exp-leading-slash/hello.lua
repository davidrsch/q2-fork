-- Minimal project-level filter: prints once per document it runs on.
function Pandoc(doc)
  print("hello from filter")
  return doc
end
