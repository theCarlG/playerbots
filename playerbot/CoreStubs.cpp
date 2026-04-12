// CoreStubs.cpp — compatibility stubs needed by the fork's core and by
// module .cpp files that haven't been ported yet.
//
// The Rust rewrite (Phases 6–8) removed the old helper .cpp files and the
// strategy-engine command handlers. The fork's Chat.cpp, however, still
// hard-references `ChatHandler::HandlePlayerbotCommand` and friends from
// its command table, and a handful of surviving module .cpp files still
// call `split()` / `strstri()`. This file provides the minimum symbols so
// the core links — each stub returns a safe default until a Rust or C++
// implementation replaces it.

#include "botpch.h"
#include "Chat/Chat.h"
#include "MgrBridge.h"
#include "RandomPlayerbotMgr.h"

#include <cstring>
#include <cctype>
#include <sstream>
#include <string>
#include <vector>

// ── Chat command stubs ──────────────────────────────────────────────────
// The fork's Chat command table takes pointers to these member functions.
// Returning `true` suppresses the "invalid command" reply while the Rust
// side does not yet implement chat-driven bot control.

bool ChatHandler::HandlePlayerbotCommand(char* args)
{
    return PlayerbotMgr::HandlePlayerbotMgrCommand(this, args);
}

bool ChatHandler::HandleRandomPlayerbotCommand(char* args)
{
    return RandomPlayerbotMgr::HandlePlayerbotConsoleCommand(this, args);
}

bool ChatHandler::HandlePerfMonCommand(char* /*args*/)
{
    return true;
}

// ── String helpers previously provided by a deleted utility .cpp ────────

std::vector<std::string> split(std::string const& s, char delim)
{
    std::vector<std::string> out;
    std::stringstream ss(s);
    std::string item;
    while (std::getline(ss, item, delim))
        out.push_back(item);
    return out;
}

void split(std::vector<std::string>& dest, std::string const& str, char const* delim)
{
    if (!delim || !*delim)
    {
        dest.push_back(str);
        return;
    }
    size_t start = 0;
    const size_t dlen = std::strlen(delim);
    while (true)
    {
        size_t pos = str.find(delim, start);
        if (pos == std::string::npos)
        {
            dest.push_back(str.substr(start));
            return;
        }
        dest.push_back(str.substr(start, pos - start));
        start = pos + dlen;
    }
}

// Case-insensitive strstr. Returns pointer into `haystack` or nullptr.
char* strstri(char const* haystack, char const* needle)
{
    if (!haystack || !needle)
        return nullptr;
    if (!*needle)
        return const_cast<char*>(haystack);

    for (char const* h = haystack; *h; ++h)
    {
        char const* hp = h;
        char const* np = needle;
        while (*hp && *np &&
               std::tolower(static_cast<unsigned char>(*hp)) ==
               std::tolower(static_cast<unsigned char>(*np)))
        {
            ++hp;
            ++np;
        }
        if (!*np)
            return const_cast<char*>(h);
    }
    return nullptr;
}
