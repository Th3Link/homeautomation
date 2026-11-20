local source = "https://github.com/Th3Link/homeautomation/blob/"
    .. (os.getenv("GITHUB_SHA") or "main") .. "/"

local function source_link(link)
    -- Both Markdown files live in docs/. Keep HTML links and page fragments local.
    if link.target:sub(1, 3) == "../" then
        link.target = source .. link.target:sub(4)
    elseif link.target:sub(1, 4) == "adr/" then
        link.target = source .. "docs/" .. link.target
    end
    return link
end

function Pandoc(handbook)
    local file = assert(io.open("docs/protocol-can.md"))
    local reference = pandoc.read(file:read("*a"), "gfm")
    file:close()
    reference = reference:walk({
        Header = function(heading)
            if heading.level == 1 then
                heading.content = {pandoc.Str("CAN wire format reference")}
                heading.identifier = "can-wire-format-reference"
            end
            heading.level = heading.level + 1
            return heading
        end,
    })
    handbook.blocks:extend(reference.blocks)
    return handbook:walk({
        Link = source_link,
        Table = function(table)
            return pandoc.Div({table}, pandoc.Attr("", {"table-scroll"}))
        end,
    })
end
