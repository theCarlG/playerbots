-- RaidControl.lua
-- Main addon logic for playerbot raid control
-- WoW 1.14.x (Classic Era) compatible

RaidControl = {}

-- Safe dropdown text setter (compatible with WoW 1.12 and 1.14)
local function SafeSetDropdownText(dropdown, text)
    local name = dropdown:GetName()
    -- 1.14 uses "Text" child; 1.12 uses "Button" child or direct method
    local textObj = _G[name .. "Text"] or _G[name .. "Button"]
    if textObj and textObj.SetText then
        textObj:SetText(text)
    elseif dropdown.SetText then
        dropdown:SetText(text)
    end
end

-- Saved variables defaults
local defaults = {
    mainTank = nil,
    offTank1 = nil,
    offTank2 = nil,
    puller = nil,
    mtHealer = nil,
    ot1Healer = nil,
    ot2Healer = nil,
    selectedRaid = 1,
    selectedBoss = 1,
    selectedTab = "assignments",
    minimized = false,
    collapsedCategories = {},
    collapsedBossCategories = {},
}

-- Local state
local currentButtons = {}
local categoryButtons = {}
local MAX_BUTTONS_PER_ROW = 4
local BUTTON_WIDTH = 88
local BUTTON_HEIGHT = 22
local BUTTON_SPACING = 4

-- Command queue for delayed execution
local commandQueue = {}
local commandTimer = 0
local commandDelay = 0

-- Substitute variables in command string
local function SubstituteVariables(command)
    local result = command

    -- Get puller name
    local pullerName = RaidControlDB.puller
    if pullerName == "MT" then
        pullerName = RaidControlDB.mainTank
    elseif pullerName == "OT1" then
        pullerName = RaidControlDB.offTank1
    elseif pullerName == "OT2" then
        pullerName = RaidControlDB.offTank2
    end

    -- Replace all placeholders (case insensitive)
    result = string.gsub(result, "{mt}", RaidControlDB.mainTank or "")
    result = string.gsub(result, "{MT}", RaidControlDB.mainTank or "")
    result = string.gsub(result, "{ot1}", RaidControlDB.offTank1 or "")
    result = string.gsub(result, "{OT1}", RaidControlDB.offTank1 or "")
    result = string.gsub(result, "{ot}", RaidControlDB.offTank1 or "")
    result = string.gsub(result, "{OT}", RaidControlDB.offTank1 or "")
    result = string.gsub(result, "{ot2}", RaidControlDB.offTank2 or "")
    result = string.gsub(result, "{OT2}", RaidControlDB.offTank2 or "")
    result = string.gsub(result, "{puller}", pullerName or "")
    result = string.gsub(result, "{PULLER}", pullerName or "")
    result = string.gsub(result, "{mth}", RaidControlDB.mtHealer or "")
    result = string.gsub(result, "{MTH}", RaidControlDB.mtHealer or "")
    result = string.gsub(result, "{ot1h}", RaidControlDB.ot1Healer or "")
    result = string.gsub(result, "{OT1H}", RaidControlDB.ot1Healer or "")
    result = string.gsub(result, "{ot2h}", RaidControlDB.ot2Healer or "")
    result = string.gsub(result, "{OT2H}", RaidControlDB.ot2Healer or "")
    result = string.gsub(result, "{player}", UnitName("player") or "")
    result = string.gsub(result, "{PLAYER}", UnitName("player") or "")

    -- Target substitution
    local targetName = ""
    if UnitExists("target") and UnitIsPlayer("target") then
        targetName = UnitName("target") or ""
    end
    result = string.gsub(result, "{target}", targetName)
    result = string.gsub(result, "{TARGET}", targetName)

    return result
end

-- Process command queue
local function ProcessCommandQueue(self, elapsed)
    if commandDelay > 0 then
        commandTimer = commandTimer + elapsed
        if commandTimer >= commandDelay then
            commandTimer = 0
            commandDelay = 0

            while commandQueue[1] do
                local cmd = commandQueue[1]
                RaidControl:ExecuteSingleCommand(cmd.command, cmd.target)
                local delayAfter = cmd.delay or 0

                for i = 1, 19 do
                    commandQueue[i] = commandQueue[i + 1]
                end
                commandQueue[20] = nil

                if commandQueue[1] and delayAfter > 0 then
                    commandDelay = delayAfter
                    commandTimer = 0
                    break
                end
            end
        end
    end
end

-- Timer frame
local timerFrame = CreateFrame("Frame", "RaidControlTimerFrame", UIParent)
timerFrame:SetScript("OnUpdate", function(self, elapsed)
    ProcessCommandQueue(self or this, elapsed or arg1)
end)

-- Execute single command
function RaidControl:ExecuteSingleCommand(command, target)
    command = SubstituteVariables(command)

    if target == "console" then
        -- Execute slash commands by calling their handlers directly
        if string.sub(command, 1, 1) == "/" then
            -- Parse: "/cast aedm" -> slashCmd="/cast", args="aedm"
            local slashCmd, args = string.match(command, "^(/[^%s]+)%s*(.*)")
            if slashCmd then
                slashCmd = string.upper(slashCmd)
                -- Find the handler for this slash command
                for cmdName, handler in pairs(SlashCmdList) do
                    -- Check SLASH_CMDNAME1, SLASH_CMDNAME2, etc.
                    for i = 1, 10 do
                        local globalName = "SLASH_" .. cmdName .. i
                        local slash = _G[globalName]
                        if slash and string.upper(slash) == slashCmd then
                            handler(args or "")
                            return
                        end
                        if not slash then break end
                    end
                end
                -- If not found in SlashCmdList, try as a chat command
                DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r Unknown command: " .. slashCmd)
            end
        end
        return
    end

    local function whisper(name, cmd)
        if name then
            SendChatMessage("BOT\tPB:v1:" .. cmd, "WHISPER", nil, name)
            return true
        end
        return false
    end

    local function chatRaid(cmd)
        local channel = IsInRaid() and "RAID" or "PARTY"
        SendAddonMessage("BOT", "PB:v1:" .. cmd, channel)
    end

    if target == "mt" then
        if not whisper(RaidControlDB.mainTank, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No MT selected!", 1, 0, 0)
        end
    elseif target == "ot" or target == "ot1" then
        if not whisper(RaidControlDB.offTank1, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No OT1 selected!", 1, 0, 0)
        end
    elseif target == "ot2" then
        if not whisper(RaidControlDB.offTank2, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No OT2 selected!", 1, 0, 0)
        end
    elseif target == "mth" then
        if not whisper(RaidControlDB.mtHealer, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No MT Healer selected!", 1, 0, 0)
        end
    elseif target == "ot1h" then
        if not whisper(RaidControlDB.ot1Healer, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No OT1 Healer selected!", 1, 0, 0)
        end
    elseif target == "ot2h" then
        if not whisper(RaidControlDB.ot2Healer, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No OT2 Healer selected!", 1, 0, 0)
        end
    elseif target == "puller" then
        local pullerName = RaidControlDB.puller
        if pullerName == "MT" then
            pullerName = RaidControlDB.mainTank
        elseif pullerName == "OT1" then
            pullerName = RaidControlDB.offTank1
        elseif pullerName == "OT2" then
            pullerName = RaidControlDB.offTank2
        end
        if not whisper(pullerName, command) then
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No Puller selected!", 1, 0, 0)
        end
    elseif target == "tanks" then
        whisper(RaidControlDB.mainTank, command)
        whisper(RaidControlDB.offTank1, command)
        whisper(RaidControlDB.offTank2, command)
    elseif target == "healers" then
        whisper(RaidControlDB.mtHealer, command)
        whisper(RaidControlDB.ot1Healer, command)
        whisper(RaidControlDB.ot2Healer, command)
    elseif target == "party" then
        SendAddonMessage("BOT", "PB:v1:" .. command, "PARTY")
    elseif target == "guild" then
        SendAddonMessage("BOT", "PB:v1:" .. command, "GUILD")
    elseif target == "target" then
        -- Whisper current target
        if UnitExists("target") and UnitIsPlayer("target") then
            local targetName = UnitName("target")
            whisper(targetName, command)
        else
            DEFAULT_CHAT_FRAME:AddMessage("|cffff0000RaidControl:|r No player targeted!", 1, 0, 0)
        end
    elseif string.sub(target, 1, 2) == "w:" then
        -- Whisper by name: "w:PlayerName"
        local name = string.sub(target, 3)
        whisper(name, command)
    elseif string.sub(target, 1, 8) == "whisper:" then
        -- Whisper by name: "whisper:PlayerName"
        local name = string.sub(target, 9)
        whisper(name, command)
    else
        chatRaid(command)
    end
end

-- Execute button commands with delay support
local function ExecuteButtonCommands(btnData)
    if btnData.commands then
        for i = 1, 20 do
            commandQueue[i] = nil
        end
        commandTimer = 0
        commandDelay = 0

        local queueIndex = 0
        for i = 1, 20 do
            local cmd = btnData.commands[i]
            if cmd then
                queueIndex = queueIndex + 1
                commandQueue[queueIndex] = {
                    command = cmd.command,
                    target = cmd.target or "raid",
                    delay = cmd.delay or 0
                }
            else
                break
            end
        end

        if commandQueue[1] then
            local cmd = commandQueue[1]
            RaidControl:ExecuteSingleCommand(cmd.command, cmd.target)
            local delayAfter = cmd.delay or 0

            for i = 1, 19 do
                commandQueue[i] = commandQueue[i + 1]
            end
            commandQueue[20] = nil

            if commandQueue[1] and delayAfter > 0 then
                commandDelay = delayAfter
                commandTimer = 0
            elseif commandQueue[1] then
                while commandQueue[1] do
                    local nextCmd = commandQueue[1]
                    RaidControl:ExecuteSingleCommand(nextCmd.command, nextCmd.target)
                    local nextDelay = nextCmd.delay or 0

                    for i = 1, 19 do
                        commandQueue[i] = commandQueue[i + 1]
                    end
                    commandQueue[20] = nil

                    if commandQueue[1] and nextDelay > 0 then
                        commandDelay = nextDelay
                        commandTimer = 0
                        break
                    end
                end
            end
        end
    elseif btnData.command then
        RaidControl:ExecuteSingleCommand(btnData.command, btnData.target or "raid")
    end
end

-- Get tank-capable members (Warrior, Druid, Paladin)
local function GetTanks()
    local tanks = {}
    local n = 0
    -- GetRaidRosterInfo returns localized class names
    local tankClasses = { Warrior = true, Druid = true, Paladin = true }
    -- UnitClass returns localized name and token
    local tankTokens = { WARRIOR = true, DRUID = true, PALADIN = true }

    if IsInRaid() then
        for i = 1, 40 do
            local name, _, _, _, class = GetRaidRosterInfo(i)
            if name and tankClasses[class] then
                n = n + 1
                tanks[n] = name
            end
        end
    elseif GetNumSubgroupMembers() > 0 then
        local _, playerClass = UnitClass("player")
        if tankTokens[playerClass] then
            n = n + 1
            tanks[n] = UnitName("player")
        end
        for i = 1, 4 do
            if UnitExists("party" .. i) then
                local _, class = UnitClass("party" .. i)
                if tankTokens[class] then
                    n = n + 1
                    tanks[n] = UnitName("party" .. i)
                end
            end
        end
    else
        local _, playerClass = UnitClass("player")
        if tankTokens[playerClass] then
            n = n + 1
            tanks[n] = UnitName("player")
        end
    end

    table.sort(tanks)
    return tanks
end

-- Get healer-capable members (Priest, Paladin)
local function GetHealers()
    local healers = {}
    local n = 0
    -- GetRaidRosterInfo returns localized class names
    local healerClasses = { Priest = true, Paladin = true }
    -- UnitClass returns localized name and token
    local healerTokens = { PRIEST = true, PALADIN = true }

    if IsInRaid() then
        for i = 1, 40 do
            local name, _, _, _, class = GetRaidRosterInfo(i)
            if name and healerClasses[class] then
                n = n + 1
                healers[n] = name
            end
        end
    elseif GetNumSubgroupMembers() > 0 then
        local _, playerClass = UnitClass("player")
        if healerTokens[playerClass] then
            n = n + 1
            healers[n] = UnitName("player")
        end
        for i = 1, 4 do
            if UnitExists("party" .. i) then
                local _, class = UnitClass("party" .. i)
                if healerTokens[class] then
                    n = n + 1
                    healers[n] = UnitName("party" .. i)
                end
            end
        end
    else
        local _, playerClass = UnitClass("player")
        if healerTokens[playerClass] then
            n = n + 1
            healers[n] = UnitName("player")
        end
    end

    table.sort(healers)
    return healers
end

-- Get hunters for puller dropdown
local function GetHunters()
    local hunters = {}
    local n = 0

    if IsInRaid() then
        for i = 1, 40 do
            local name, _, _, _, class = GetRaidRosterInfo(i)
            if name and class == "Hunter" then
                n = n + 1
                hunters[n] = name
            end
        end
    elseif GetNumSubgroupMembers() > 0 then
        local _, playerClass = UnitClass("player")
        if playerClass == "HUNTER" then
            n = n + 1
            hunters[n] = UnitName("player")
        end
        for i = 1, 4 do
            if UnitExists("party" .. i) then
                local _, class = UnitClass("party" .. i)
                if class == "HUNTER" then
                    n = n + 1
                    hunters[n] = UnitName("party" .. i)
                end
            end
        end
    end

    table.sort(hunters)
    return hunters
end

-- Tank role dropdown (Warrior, Druid, Paladin only)
local function MakeTankDropdownInit(dropdown, roleKey)
    return function()
        local tanks = GetTanks()
        local info

        info = {}
        info.text = "-- None --"
        info.func = function()
            RaidControlDB[roleKey] = nil
            SafeSetDropdownText(dropdown, "-- None --")
        end
        info.checked = (RaidControlDB[roleKey] == nil)
        UIDropDownMenu_AddButton(info)

        for i = 1, 40 do
            local name = tanks[i]
            if name then
                local capturedName = name
                info = {}
                info.text = capturedName
                info.func = function()
                    RaidControlDB[roleKey] = capturedName
                    SafeSetDropdownText(dropdown, capturedName)
                end
                info.checked = (RaidControlDB[roleKey] == capturedName)
                UIDropDownMenu_AddButton(info)
            else
                break
            end
        end
    end
end

-- Healer role dropdown (Priest, Paladin only)
local function MakeHealerDropdownInit(dropdown, roleKey)
    return function()
        local healers = GetHealers()
        local info

        info = {}
        info.text = "-- None --"
        info.func = function()
            RaidControlDB[roleKey] = nil
            SafeSetDropdownText(dropdown, "-- None --")
        end
        info.checked = (RaidControlDB[roleKey] == nil)
        UIDropDownMenu_AddButton(info)

        for i = 1, 40 do
            local name = healers[i]
            if name then
                local capturedName = name
                info = {}
                info.text = capturedName
                info.func = function()
                    RaidControlDB[roleKey] = capturedName
                    SafeSetDropdownText(dropdown, capturedName)
                end
                info.checked = (RaidControlDB[roleKey] == capturedName)
                UIDropDownMenu_AddButton(info)
            else
                break
            end
        end
    end
end

local function InitializePullerDropdown()
    local info

    info = {}
    info.text = "-- None --"
    info.func = function()
        RaidControlDB.puller = nil
        SafeSetDropdownText(RaidControlPullerDropdown, "-- None --")
    end
    info.checked = (RaidControlDB.puller == nil)
    UIDropDownMenu_AddButton(info)

    -- Tank options
    local tankOpts = { "MT", "OT1", "OT2" }
    local tankLabels = { "Main Tank", "Off Tank 1", "Off Tank 2" }
    for i = 1, 3 do
        local val = tankOpts[i]
        local label = tankLabels[i]
        info = {}
        info.text = "|cffff8800" .. val .. "|r (" .. label .. ")"
        info.func = function()
            RaidControlDB.puller = val
            SafeSetDropdownText(RaidControlPullerDropdown, val)
        end
        info.checked = (RaidControlDB.puller == val)
        UIDropDownMenu_AddButton(info)
    end

    -- Hunters
    local hunters = GetHunters()
    for i = 1, 20 do
        local name = hunters[i]
        if name then
            local capturedName = name
            info = {}
            info.text = "|cffabd473" .. capturedName .. "|r (Hunter)"
            info.func = function()
                RaidControlDB.puller = capturedName
                SafeSetDropdownText(RaidControlPullerDropdown, capturedName)
            end
            info.checked = (RaidControlDB.puller == capturedName)
            UIDropDownMenu_AddButton(info)
        else
            break
        end
    end
end

local function InitializeRaidDropdown()
    for i = 1, 20 do
        local raid = RaidControl_BossData.raids[i]
        if raid then
            local idx = i
            local raidName = raid.name
            local info = {}
            info.text = raidName
            info.func = function()
                RaidControlDB.selectedRaid = idx
                RaidControlDB.selectedBoss = 1
                SafeSetDropdownText(RaidControlRaidDropdown, raidName)
                local newRaid = RaidControl_BossData.raids[idx]
                if newRaid and newRaid.bosses[1] then
                    SafeSetDropdownText(RaidControlBossDropdown, newRaid.bosses[1].name)
                end
                RaidControl:RefreshAllButtons()
                -- Reset scroll position
                if RaidControlScrollBar then
                    RaidControlScrollBar:SetValue(0)
                end
            end
            info.checked = (RaidControlDB.selectedRaid == idx)
            UIDropDownMenu_AddButton(info)
        else
            break
        end
    end
end

local function InitializeBossDropdown()
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    if not raid then return end

    for i = 1, 30 do
        local boss = raid.bosses[i]
        if boss then
            local idx = i
            local bossName = boss.name
            local info = {}
            info.text = bossName
            info.func = function()
                RaidControlDB.selectedBoss = idx
                SafeSetDropdownText(RaidControlBossDropdown, bossName)
                RaidControl:RefreshAllButtons()
                -- Reset scroll position
                if RaidControlScrollBar then
                    RaidControlScrollBar:SetValue(0)
                end
            end
            info.checked = (RaidControlDB.selectedBoss == idx)
            UIDropDownMenu_AddButton(info)
        else
            break
        end
    end
end

-- Build tooltip text
local function BuildTooltipText(btnData)
    local lines = {}
    local n = 0

    if btnData.commands then
        for i = 1, 20 do
            local cmd = btnData.commands[i]
            if cmd then
                n = n + 1
                lines[n] = {
                    command = cmd.command,
                    target = cmd.target or "raid",
                    delay = cmd.delay or 0
                }
            else
                break
            end
        end
    elseif btnData.command then
        n = 1
        lines[1] = {
            command = btnData.command,
            target = btnData.target or "raid",
            delay = 0
        }
    end

    lines.n = n
    return lines
end

-- Check if button data contains a spell cast
local function GetSpellFromCommands(btnData)
    if btnData.commands then
        for i = 1, 20 do
            local cmd = btnData.commands[i]
            if cmd and cmd.target == "spell" then
                return cmd.command, i
            end
            if not cmd then break end
        end
    elseif btnData.target == "spell" then
        return btnData.command, 1
    end
    return nil, nil
end

-- Execute non-spell commands (for PreClick)
local function ExecuteNonSpellCommands(btnData)
    if btnData.commands then
        for i = 1, 20 do
            local cmd = btnData.commands[i]
            if cmd then
                if cmd.target ~= "spell" then
                    local target = cmd.target or "raid"
                    local command = cmd.command
                    local delay = cmd.delay or 0
                    
                    if command then
                        if delay > 0 then
                            local queueIdx = table.getn(commandQueue) + 1
                            commandQueue[queueIdx] = { target = target, command = command, delay = delay }
                        else
                            RaidControl:ExecuteSingleCommand(command, target)
                        end
                    end
                end
            else
                break
            end
        end
        -- Start timer if there are delayed commands
        if table.getn(commandQueue) > 0 then
            commandTimer = 0
            commandDelay = commandQueue[1].delay
        end
    end
end

-- Create command button with modern flat style
local function CreateCommandButton(parent, name)
    local btn = CreateFrame("Button", name, parent, RC_NATIVE_CLEU and "SecureActionButtonTemplate" or nil)
    btn:SetWidth(BUTTON_WIDTH)
    btn:SetHeight(BUTTON_HEIGHT)
    btn:SetText("")
    btn:Hide()
    
    -- Modern flat background
    local WHITE = "Interface\\Buttons\\WHITE8x8"
    local bg = btn:CreateTexture(nil, "BACKGROUND")
    bg:SetTexture(WHITE)
    bg:SetAllPoints(btn)
    bg:SetVertexColor(0.2, 0.2, 0.2, 0.9)
    btn.bg = bg

    -- Highlight texture
    local hl = btn:CreateTexture(nil, "HIGHLIGHT")
    hl:SetTexture(WHITE)
    hl:SetAllPoints(btn)
    hl:SetVertexColor(0.4, 0.4, 0.4, 0.4)

    -- Border
    local border = btn:CreateTexture(nil, "BORDER")
    border:SetTexture(WHITE)
    border:SetPoint("TOPLEFT", btn, "TOPLEFT", -1, 1)
    border:SetPoint("BOTTOMRIGHT", btn, "BOTTOMRIGHT", 1, -1)
    border:SetVertexColor(0.3, 0.3, 0.3, 1)
    btn.border = border
    
    -- Text
    local text = btn:CreateFontString(nil, "OVERLAY", "GameFontNormalSmall")
    text:SetPoint("CENTER", btn, "CENTER", 0, 0)
    text:SetTextColor(1, 1, 1)
    btn:SetFontString(text)
    btn.text = text

    if RC_NATIVE_CLEU then
        -- 1.14: SecureActionButtonTemplate supports PreClick; spell cast goes through the secure action
        btn:SetScript("PreClick", function(self)
            self = self or this
            if self.btnData then
                local spell = GetSpellFromCommands(self.btnData)
                if spell then
                    ExecuteNonSpellCommands(self.btnData)
                else
                    ExecuteButtonCommands(self.btnData)
                end
            end
        end)
    else
        -- 1.12: plain Button, no PreClick — run all commands on OnClick (no secure spell cast)
        btn:SetScript("OnClick", function(self)
            self = self or this
            if self.btnData then
                ExecuteButtonCommands(self.btnData)
            end
        end)
    end

    btn:SetScript("OnEnter", function(self)
        self = self or this
        -- Highlight effect
        self.bg:SetVertexColor(0.3, 0.3, 0.3, 0.95)
        
        if self.btnData then
            GameTooltip:SetOwner(UIParent, "ANCHOR_CURSOR")
            local lines = BuildTooltipText(self.btnData)
            if lines.n == 1 and lines[1] then
                local cmd = lines[1].command or ""
                local tgt = lines[1].target or "raid"
                if tgt == "spell" then
                    GameTooltip:SetText("Cast: " .. cmd, 1, 0.5, 1)
                else
                    GameTooltip:SetText(cmd, 1, 1, 1)
                    GameTooltip:AddLine("Target: " .. tgt, 0.7, 0.7, 0.7)
                end
            elseif lines.n > 1 then
                GameTooltip:SetText("Multiple Commands:", 1, 0.82, 0)
                for i = 1, lines.n do
                    local line = lines[i]
                    if line then
                        local cmd = line.command or ""
                        local tgt = line.target or "raid"
                        local delay = line.delay or 0
                        local text
                        if tgt == "spell" then
                            text = i .. ". |cffff88ffCast: " .. cmd .. "|r"
                        else
                            text = i .. ". [" .. tgt .. "] " .. cmd
                        end
                        if delay > 0 then
                            text = text .. " |cff888888(+" .. delay .. "s)|r"
                        end
                        GameTooltip:AddLine(text, 1, 1, 1)
                    end
                end
            else
                GameTooltip:SetText(self.btnData.label or "Button", 1, 1, 1)
            end
            GameTooltip:Show()
        end
    end)

    btn:SetScript("OnLeave", function(self)
        self = self or this
        -- Reset highlight
        self.bg:SetVertexColor(0.2, 0.2, 0.2, 0.9)
        GameTooltip:Hide()
    end)

    return btn
end

-- Configure button for spell casting (SecureActionButtonTemplate only, 1.14+)
local function ConfigureButtonSpell(btn)
    if not RC_NATIVE_CLEU then return end  -- no secure frames in 1.12
    local spell = GetSpellFromCommands(btn.btnData)
    if spell then
        btn:SetAttribute("type", "spell")
        btn:SetAttribute("spell", spell)
    else
        btn:SetAttribute("type", nil)
        btn:SetAttribute("spell", nil)
    end
end

-- Category header buttons and labels
local categoryHeaders = {}
local bossHeaders = {}

-- Refresh all buttons
function RaidControl:RefreshAllButtons()
    -- Hide existing buttons and headers
    for i = 1, 100 do
        if categoryButtons[i] then
            categoryButtons[i]:Hide()
        end
        if currentButtons[i] then
            currentButtons[i]:Hide()
        end
        if categoryHeaders[i] then
            categoryHeaders[i]:Hide()
        end
        if bossHeaders[i] then
            bossHeaders[i]:Hide()
        end
    end

    -- Ensure collapsed tables exist
    if not RaidControlDB.collapsedCategories then
        RaidControlDB.collapsedCategories = {}
    end
    if not RaidControlDB.collapsedBossCategories then
        RaidControlDB.collapsedBossCategories = {}
    end

    -- Layout category buttons grouped by category
    local btnIndex = 0
    local headerIndex = 0
    local yOffset = 0

    if RaidControl_Categories then
        for i = 1, 20 do
            local category = RaidControl_Categories[i]
            if category and category.buttons then
                -- Count buttons in this category
                local btnCount = 0
                for j = 1, 20 do
                    if category.buttons[j] then
                        btnCount = btnCount + 1
                    else
                        break
                    end
                end

                if btnCount > 0 then
                    local catName = category.name
                    local isCollapsible = category.collapsible
                    local isCollapsed = isCollapsible and RaidControlDB.collapsedCategories[catName]

                    -- Create/show category header button
                    headerIndex = headerIndex + 1
                    if not categoryHeaders[headerIndex] then
                        categoryHeaders[headerIndex] = CreateFrame("Button", "RaidControlCatHeader" .. headerIndex,
                            RaidControlCategoryFrame)
                        categoryHeaders[headerIndex]:SetHeight(14)
                        categoryHeaders[headerIndex].text = categoryHeaders[headerIndex]:CreateFontString(nil, "OVERLAY",
                            "GameFontNormalSmall")
                        categoryHeaders[headerIndex].text:SetPoint("LEFT", categoryHeaders[headerIndex], "LEFT", 0, 0)
                    end

                    local header = categoryHeaders[headerIndex]
                    local headerText
                    if isCollapsible then
                        local suffix = isCollapsed and "+" or "-"
                        headerText = catName .. " |cff00ff00" .. suffix .. "|r"
                        header:SetHighlightTexture("Interface\\QuestFrame\\UI-QuestTitleHighlight", "ADD")
                        header:EnableMouse(true)
                    else
                        headerText = "|cffffd100" .. catName .. "|r"
                        header:SetHighlightTexture(nil)
                        header:EnableMouse(false)
                    end
                    header.text:SetText(headerText)
                    header:SetWidth(header.text:GetStringWidth() + 10)
                    header:ClearAllPoints()
                    header:SetPoint("TOPLEFT", RaidControlCategoryFrame, "TOPLEFT", 0, yOffset)
                    header.categoryName = catName
                    header.isCollapsible = isCollapsible
                    header:SetScript("OnClick", function(self)
                        self = self or this
                        if self.isCollapsible then
                            RaidControlDB.collapsedCategories[self.categoryName] = not RaidControlDB.collapsedCategories[self.categoryName]
                            RaidControl:RefreshAllButtons()
                        end
                    end)
                    header:Show()

                    yOffset = yOffset - 14 -- Space after header

                    -- Only show buttons if not collapsed
                    if not isCollapsed then
                        local col = 0
                        for j = 1, btnCount do
                            local btnData = category.buttons[j]
                            btnIndex = btnIndex + 1

                            if not categoryButtons[btnIndex] then
                                categoryButtons[btnIndex] = CreateCommandButton(RaidControlCategoryFrame,
                                    "RaidControlCatBtn" .. btnIndex)
                            end

                            local btn = categoryButtons[btnIndex]
                            btn:SetText(btnData.label)
                            btn.btnData = btnData
                            ConfigureButtonSpell(btn)

                            local xOffset = col * (BUTTON_WIDTH + BUTTON_SPACING)

                            btn:ClearAllPoints()
                            btn:SetPoint("TOPLEFT", RaidControlCategoryFrame, "TOPLEFT", xOffset, yOffset)
                            btn:Show()

                            col = col + 1
                            if col >= MAX_BUTTONS_PER_ROW then
                                col = 0
                                yOffset = yOffset - (BUTTON_HEIGHT + BUTTON_SPACING)
                            end
                        end

                        -- Move to next row after category (if we didn't just wrap)
                        if col > 0 then
                            yOffset = yOffset - (BUTTON_HEIGHT + BUTTON_SPACING)
                        end
                    end

                    -- Add small gap between categories
                    yOffset = yOffset - 4
                end
            elseif not category then
                break
            end
        end
    end

    -- Resize category frame
    local catHeight = -yOffset
    if catHeight < 28 then catHeight = 28 end
    RaidControlCategoryFrame:SetHeight(catHeight)

    -- Boss-specific buttons (now with categories)
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    local boss = raid and raid.bosses[RaidControlDB.selectedBoss]

    -- Create boss category key for tracking collapsed state
    local bossKey = RaidControlDB.selectedRaid .. "-" .. RaidControlDB.selectedBoss
    if not RaidControlDB.collapsedBossCategories[bossKey] then
        RaidControlDB.collapsedBossCategories[bossKey] = {}
    end

    local bossBtnIndex = 0
    local bossHeaderIndex = 0
    local bossYOffset = 0

    if boss and boss.categories then
        for i = 1, 20 do
            local category = boss.categories[i]
            if category and category.buttons then
                -- Count buttons in this category
                local btnCount = 0
                for j = 1, 20 do
                    if category.buttons[j] then
                        btnCount = btnCount + 1
                    else
                        break
                    end
                end

                if btnCount > 0 then
                    local catName = category.name
                    local isCollapsible = category.collapsible
                    local isCollapsed = isCollapsible and RaidControlDB.collapsedBossCategories[bossKey][catName]

                    -- Create/show category header button
                    bossHeaderIndex = bossHeaderIndex + 1
                    if not bossHeaders[bossHeaderIndex] then
                        bossHeaders[bossHeaderIndex] = CreateFrame("Button", "RaidControlBossHeader" .. bossHeaderIndex,
                            RaidControlBossButtonFrame)
                        bossHeaders[bossHeaderIndex]:SetHeight(14)
                        bossHeaders[bossHeaderIndex].text = bossHeaders[bossHeaderIndex]:CreateFontString(nil, "OVERLAY",
                            "GameFontNormalSmall")
                        bossHeaders[bossHeaderIndex].text:SetPoint("LEFT", bossHeaders[bossHeaderIndex], "LEFT", 0, 0)
                    end

                    local header = bossHeaders[bossHeaderIndex]
                    local headerText
                    if isCollapsible then
                        local suffix = isCollapsed and "+" or "-"
                        headerText = catName .. " |cff00ff00" .. suffix .. "|r"
                        header:SetHighlightTexture("Interface\\QuestFrame\\UI-QuestTitleHighlight", "ADD")
                        header:EnableMouse(true)
                    else
                        headerText = "|cffffd100" .. catName .. "|r"
                        header:SetHighlightTexture(nil)
                        header:EnableMouse(false)
                    end
                    header.text:SetText(headerText)
                    header:SetWidth(header.text:GetStringWidth() + 10)
                    header:ClearAllPoints()
                    header:SetPoint("TOPLEFT", RaidControlBossButtonFrame, "TOPLEFT", 0, bossYOffset)
                    header.categoryName = catName
                    header.bossKey = bossKey
                    header.isCollapsible = isCollapsible
                    header:SetScript("OnClick", function(self)
                        self = self or this
                        if self.isCollapsible then
                            RaidControlDB.collapsedBossCategories[self.bossKey][self.categoryName] = not RaidControlDB.collapsedBossCategories[self.bossKey][self.categoryName]
                            RaidControl:RefreshAllButtons()
                        end
                    end)
                    header:Show()

                    bossYOffset = bossYOffset - 14 -- Space after header

                    -- Only show buttons if not collapsed
                    if not isCollapsed then
                        local col = 0
                        for j = 1, btnCount do
                            local btnData = category.buttons[j]
                            bossBtnIndex = bossBtnIndex + 1

                            if not currentButtons[bossBtnIndex] then
                                currentButtons[bossBtnIndex] = CreateCommandButton(RaidControlBossButtonFrame,
                                    "RaidControlBossBtn" .. bossBtnIndex)
                            end

                            local btn = currentButtons[bossBtnIndex]
                            btn:SetText(btnData.label)
                            btn.btnData = btnData
                            ConfigureButtonSpell(btn)

                            local xOffset = col * (BUTTON_WIDTH + BUTTON_SPACING)

                            btn:ClearAllPoints()
                            btn:SetPoint("TOPLEFT", RaidControlBossButtonFrame, "TOPLEFT", xOffset, bossYOffset)
                            btn:Show()

                            col = col + 1
                            if col >= MAX_BUTTONS_PER_ROW then
                                col = 0
                                bossYOffset = bossYOffset - (BUTTON_HEIGHT + BUTTON_SPACING)
                            end
                        end

                        -- Move to next row after category (if we didn't just wrap)
                        if col > 0 then
                            bossYOffset = bossYOffset - (BUTTON_HEIGHT + BUTTON_SPACING)
                        end
                    end

                    -- Add small gap between categories
                    bossYOffset = bossYOffset - 4
                end
            elseif not category then
                break
            end
        end
    end

    -- Resize boss frame
    local bossHeight = -bossYOffset
    if bossHeight < 28 then bossHeight = 28 end
    RaidControlBossButtonFrame:SetHeight(bossHeight)
    
    -- Update boss frame visibility
    if bossHeaderIndex == 0 then
        RaidControlBossButtonFrame:Hide()
    else
        RaidControlBossButtonFrame:Show()
    end
    
    -- Calculate total content height for scroll frame
    -- Category height + RaidBossRow (50) + gap (5) + Boss buttons height
    local totalContentHeight = catHeight + 50 + 5 + bossHeight + 20  -- extra padding at bottom
    RaidControl:UpdateScrollFrame(totalContentHeight)
end

-- Show tab
function RaidControl:ShowTab(tabName)
    RaidControlDB.selectedTab = tabName

    if tabName == "assignments" then
        RaidControlAssignmentsTab:Show()
        RaidControlControlsTab:Hide()
        -- Active tab styling
        RaidControlTabAssignments:SetText("|cffffffff[Assignments]|r")
        if RaidControlTabAssignments.tabBg then RaidControlTabAssignments.tabBg:SetVertexColor(0.3, 0.3, 0.5, 0.95) end
        RaidControlTabControls:SetText("|cff888888Controls|r")
        if RaidControlTabControls.tabBg then RaidControlTabControls.tabBg:SetVertexColor(0.2, 0.2, 0.2, 0.9) end
    else
        RaidControlAssignmentsTab:Hide()
        RaidControlControlsTab:Show()
        -- Active tab styling
        RaidControlTabAssignments:SetText("|cff888888Assignments|r")
        if RaidControlTabAssignments.tabBg then RaidControlTabAssignments.tabBg:SetVertexColor(0.2, 0.2, 0.2, 0.9) end
        RaidControlTabControls:SetText("|cffffffff[Controls]|r")
        if RaidControlTabControls.tabBg then RaidControlTabControls.tabBg:SetVertexColor(0.3, 0.3, 0.5, 0.95) end
    end
end

-- Run raid setup commands
function RaidControl:RunRaidSetup()
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    if not raid then
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: No raid selected")
        return
    end

    if not raid.setup then
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: No setup commands for " .. raid.name)
        return
    end

    DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Running setup for " .. raid.name)

    -- Execute setup commands
    for i = 1, 50 do
        local cmd = raid.setup[i]
        if cmd then
            local target = cmd.target or "raid"
            local command = cmd.command
            local delay = cmd.delay or 0

            if command then
                command = SubstituteVariables(command)
                if delay > 0 then
                    local queueIdx = table.getn(commandQueue) + 1
                    commandQueue[queueIdx] = { target = target, command = command, delay = delay }
                else
                    RaidControl:ExecuteSingleCommand(command, target)
                end
            end
        else
            break
        end
    end

    -- Start timer if there are delayed commands
    if table.getn(commandQueue) > 0 then
        commandTimer = 0
        commandDelay = commandQueue[1].delay
        RaidControlFrame:SetScript("OnUpdate", ProcessCommandQueue)
    end
end

-- Setup button tooltip
function RaidControl:SetupButtonEnter(btn)
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    
    GameTooltip:SetOwner(UIParent, "ANCHOR_CURSOR")
    GameTooltip:SetText("Run Raid Setup", 0.2, 1, 0.2)
    
    if raid and raid.setup then
        GameTooltip:AddLine(" ")
        GameTooltip:AddLine("Setup commands for " .. raid.name .. ":", 1, 0.82, 0)
        
        local cmdCount = 0
        for i = 1, 50 do
            if raid.setup[i] then
                cmdCount = cmdCount + 1
                if cmdCount <= 8 then  -- Show first 8 commands
                    local cmd = raid.setup[i]
                    local target = cmd.target or "raid"
                    local text = "[" .. target .. "] " .. (cmd.command or "")
                    if cmd.delay and cmd.delay > 0 then
                        text = text .. " |cff888888(+" .. cmd.delay .. "s)|r"
                    end
                    GameTooltip:AddLine(text, 0.8, 0.8, 0.8)
                end
            else
                break
            end
        end
        
        if cmdCount > 8 then
            GameTooltip:AddLine("... and " .. (cmdCount - 8) .. " more", 0.5, 0.5, 0.5)
        end
        
        if cmdCount == 0 then
            GameTooltip:AddLine("(No commands defined)", 0.5, 0.5, 0.5)
        end
    else
        GameTooltip:AddLine("No setup defined for this raid", 1, 0.3, 0.3)
    end
    
    GameTooltip:AddLine(" ")
    GameTooltip:AddLine("Also available via: /rc setup", 0.5, 0.5, 0.5)
    GameTooltip:Show()
end

-- Toggle minimize
function RaidControl:ToggleMinimize()
    RaidControlDB.minimized = not RaidControlDB.minimized

    if RaidControlDB.minimized then
        RaidControlContent:Hide()
        RaidControlTabAssignments:Hide()
        RaidControlTabControls:Hide()
        RaidControlFrame:SetHeight(30)
    else
        RaidControlContent:Show()
        RaidControlTabAssignments:Show()
        RaidControlTabControls:Show()
        RaidControlFrame:SetHeight(500)
    end
end

-- Style buttons with modern look
local function StyleButton(btn)
    if not btn then return end
    -- Make the XML NormalTexture transparent so it doesn't obscure button text
    -- (in some WoW builds the NormalTexture renders at or above the text layer).
    local nt = btn.GetNormalTexture and btn:GetNormalTexture()
    if nt then nt:SetVertexColor(0, 0, 0, 0) end
    -- Dark background at BACKGROUND layer — guaranteed below the OVERLAY text.
    if not btn.tabBg then
        local tabBg = btn:CreateTexture(nil, "BACKGROUND")
        tabBg:SetTexture("Interface\\Buttons\\WHITE8x8")
        tabBg:SetAllPoints(btn)
        tabBg:SetVertexColor(0.2, 0.2, 0.2, 0.9)
        btn.tabBg = tabBg
    end
end

-- OnLoad
function RaidControl:OnLoad(frame)
    if RC_NATIVE_CLEU then
        frame:RegisterEvent("ADDON_LOADED")               -- 1.14: early per-addon init
        frame:RegisterEvent("GROUP_ROSTER_UPDATE")        -- 1.14: roster changes
        frame:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED") -- 1.14: trigger backend
    end
    frame:RegisterEvent("PLAYER_ENTERING_WORLD")          -- DB init fallback + dropdown init
    frame:RegisterEvent("PARTY_MEMBERS_CHANGED")          -- 1.12 roster changes
    frame:RegisterEvent("RAID_ROSTER_UPDATE")             -- 1.12 roster changes
    frame:SetMovable(true)
    frame:EnableMouse(true)
    frame:RegisterForDrag("LeftButton")
    
    -- Create background texture (WHITE8x8 + SetVertexColor works in all WoW versions)
    local WHITE = "Interface\\Buttons\\WHITE8x8"
    local bg = frame:CreateTexture(nil, "BACKGROUND")
    bg:SetTexture(WHITE)
    bg:SetAllPoints(frame)
    bg:SetVertexColor(0.08, 0.08, 0.1, 0.95)

    -- Create border textures
    local borderSize = 2
    local br, bg2, bb, ba = 0.4, 0.4, 0.45, 1

    local topBorder = frame:CreateTexture(nil, "BORDER")
    topBorder:SetTexture(WHITE)
    topBorder:SetPoint("TOPLEFT", frame, "TOPLEFT", 0, 0)
    topBorder:SetPoint("TOPRIGHT", frame, "TOPRIGHT", 0, 0)
    topBorder:SetHeight(borderSize)
    topBorder:SetVertexColor(br, bg2, bb, ba)

    local bottomBorder = frame:CreateTexture(nil, "BORDER")
    bottomBorder:SetTexture(WHITE)
    bottomBorder:SetPoint("BOTTOMLEFT", frame, "BOTTOMLEFT", 0, 0)
    bottomBorder:SetPoint("BOTTOMRIGHT", frame, "BOTTOMRIGHT", 0, 0)
    bottomBorder:SetHeight(borderSize)
    bottomBorder:SetVertexColor(br, bg2, bb, ba)

    local leftBorder = frame:CreateTexture(nil, "BORDER")
    leftBorder:SetTexture(WHITE)
    leftBorder:SetPoint("TOPLEFT", frame, "TOPLEFT", 0, 0)
    leftBorder:SetPoint("BOTTOMLEFT", frame, "BOTTOMLEFT", 0, 0)
    leftBorder:SetWidth(borderSize)
    leftBorder:SetVertexColor(br, bg2, bb, ba)

    local rightBorder = frame:CreateTexture(nil, "BORDER")
    rightBorder:SetTexture(WHITE)
    rightBorder:SetPoint("TOPRIGHT", frame, "TOPRIGHT", 0, 0)
    rightBorder:SetPoint("BOTTOMRIGHT", frame, "BOTTOMRIGHT", 0, 0)
    rightBorder:SetWidth(borderSize)
    rightBorder:SetVertexColor(br, bg2, bb, ba)

    -- Title bar with accent color
    local titleBar = frame:CreateTexture(nil, "ARTWORK")
    titleBar:SetTexture(WHITE)
    titleBar:SetPoint("TOPLEFT", frame, "TOPLEFT", borderSize, -borderSize)
    titleBar:SetPoint("TOPRIGHT", frame, "TOPRIGHT", -borderSize, -borderSize)
    titleBar:SetHeight(28)
    titleBar:SetVertexColor(0.15, 0.15, 0.2, 1)
    
    -- Style the title
    RaidControlTitle:SetFont("Fonts\\FRIZQT__.TTF", 14, "OUTLINE")
    RaidControlTitle:SetTextColor(1, 0.82, 0)
    
    -- Style tab buttons
    StyleButton(RaidControlTabAssignments)
    StyleButton(RaidControlTabControls)
    StyleButton(RaidControlMinButton)
    StyleButton(RaidControlSetupButton)
    
    RaidControlMinButton:SetText("-")
    RaidControlSetupButton:SetText("Setup")
    
    -- Enable mouse wheel on scroll frame (required in WoW 1.12)
    RaidControlScrollFrame:EnableMouseWheel(true)

    -- Initialize scrollbar
    RaidControlScrollBar:SetMinMaxValues(0, 1)
    RaidControlScrollBar:SetValue(0)
    RaidControlScrollBar:SetValueStep(20)
    
    -- Scrollbar background
    local scrollBg = RaidControlScrollBar:CreateTexture(nil, "BACKGROUND")
    scrollBg:SetTexture("Interface\\Buttons\\WHITE8x8")
    scrollBg:SetAllPoints(RaidControlScrollBar)
    scrollBg:SetVertexColor(0.1, 0.1, 0.1, 0.6)

    -- Scrollbar thumb colour (XML <Color> tag ignored in 1.12)
    local thumb = _G["RaidControlScrollBarThumb"]
    if thumb then
        thumb:SetTexture("Interface\\Buttons\\WHITE8x8")
        thumb:SetVertexColor(0.5, 0.5, 0.5, 0.9)
    end
end

-- Scroll wheel handler
function RaidControl:OnScrollWheel(scrollFrame, delta)
    local scrollBar = RaidControlScrollBar
    local min, max = scrollBar:GetMinMaxValues()
    local current = scrollBar:GetValue()
    local step = 30  -- pixels per scroll
    
    local newValue = current - (delta * step)
    newValue = math.max(min, math.min(max, newValue))
    scrollBar:SetValue(newValue)
end

-- Scrollbar value changed handler
function RaidControl:OnScrollValueChanged(scrollBar, value)
    local scrollFrame = RaidControlScrollFrame
    if scrollFrame then
        scrollFrame:SetVerticalScroll(value)
    end
end

-- Update scroll frame based on content height
function RaidControl:UpdateScrollFrame(contentHeight)
    local scrollFrame = RaidControlScrollFrame
    local scrollChild = RaidControlScrollChild
    local scrollBar = RaidControlScrollBar
    
    if not scrollFrame or not scrollChild or not scrollBar then return end
    
    -- Set scroll child height
    scrollChild:SetHeight(contentHeight)
    
    -- Calculate max scroll value
    local frameHeight = scrollFrame:GetHeight()
    local maxScroll = math.max(0, contentHeight - frameHeight)
    
    -- Update scrollbar
    scrollBar:SetMinMaxValues(0, maxScroll)
    
    -- Show/hide scrollbar based on need
    if maxScroll > 0 then
        scrollBar:Show()
        -- Update thumb size proportionally
        local thumbHeight = math.max(30, (frameHeight / contentHeight) * frameHeight)
        local thumb = _G["RaidControlScrollBarThumb"]
        if thumb then
            thumb:SetHeight(thumbHeight)
        end
    else
        scrollBar:Hide()
        scrollBar:SetValue(0)
    end
end

-- Initialize dropdowns (called after UI is ready)
local dropdownsInitialized = false
local function InitializeAllDropdowns()
    if dropdownsInitialized then return end
    dropdownsInitialized = true

    -- Tank roles (Warrior, Druid, Paladin)
    UIDropDownMenu_Initialize(RaidControlMTDropdown, MakeTankDropdownInit(RaidControlMTDropdown, "mainTank"))
    UIDropDownMenu_SetWidth(RaidControlMTDropdown, 90)
    SafeSetDropdownText(RaidControlMTDropdown, RaidControlDB.mainTank or "MT")

    UIDropDownMenu_Initialize(RaidControlOT1Dropdown, MakeTankDropdownInit(RaidControlOT1Dropdown, "offTank1"))
    UIDropDownMenu_SetWidth(RaidControlOT1Dropdown, 90)
    SafeSetDropdownText(RaidControlOT1Dropdown, RaidControlDB.offTank1 or "OT1")

    UIDropDownMenu_Initialize(RaidControlOT2Dropdown, MakeTankDropdownInit(RaidControlOT2Dropdown, "offTank2"))
    UIDropDownMenu_SetWidth(RaidControlOT2Dropdown, 90)
    SafeSetDropdownText(RaidControlOT2Dropdown, RaidControlDB.offTank2 or "OT2")

    -- Puller (tanks or hunters)
    UIDropDownMenu_Initialize(RaidControlPullerDropdown, InitializePullerDropdown)
    UIDropDownMenu_SetWidth(RaidControlPullerDropdown, 90)
    SafeSetDropdownText(RaidControlPullerDropdown, RaidControlDB.puller or "Puller")

    -- Healer roles (Priest, Paladin)
    UIDropDownMenu_Initialize(RaidControlMTHDropdown, MakeHealerDropdownInit(RaidControlMTHDropdown, "mtHealer"))
    UIDropDownMenu_SetWidth(RaidControlMTHDropdown, 90)
    SafeSetDropdownText(RaidControlMTHDropdown, RaidControlDB.mtHealer or "MT Healer")

    UIDropDownMenu_Initialize(RaidControlOT1HDropdown, MakeHealerDropdownInit(RaidControlOT1HDropdown, "ot1Healer"))
    UIDropDownMenu_SetWidth(RaidControlOT1HDropdown, 90)
    SafeSetDropdownText(RaidControlOT1HDropdown, RaidControlDB.ot1Healer or "OT1 Healer")

    UIDropDownMenu_Initialize(RaidControlOT2HDropdown, MakeHealerDropdownInit(RaidControlOT2HDropdown, "ot2Healer"))
    UIDropDownMenu_SetWidth(RaidControlOT2HDropdown, 90)
    SafeSetDropdownText(RaidControlOT2HDropdown, RaidControlDB.ot2Healer or "OT2 Healer")

    UIDropDownMenu_Initialize(RaidControlRaidDropdown, InitializeRaidDropdown)
    UIDropDownMenu_SetWidth(RaidControlRaidDropdown, 120)
    local raidName = "Select Raid"
    if RaidControl_BossData.raids[RaidControlDB.selectedRaid] then
        raidName = RaidControl_BossData.raids[RaidControlDB.selectedRaid].name
    end
    SafeSetDropdownText(RaidControlRaidDropdown, raidName)

    UIDropDownMenu_Initialize(RaidControlBossDropdown, InitializeBossDropdown)
    UIDropDownMenu_SetWidth(RaidControlBossDropdown, 120)
    local bossName = "Select Boss"
    local selRaid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    if selRaid and selRaid.bosses[RaidControlDB.selectedBoss] then
        bossName = selRaid.bosses[RaidControlDB.selectedBoss].name
    end
    SafeSetDropdownText(RaidControlBossDropdown, bossName)

    -- Apply tab state
    RaidControl:ShowTab(RaidControlDB.selectedTab or "assignments")

    if RaidControlDB.minimized then
        RaidControlContent:Hide()
        RaidControlTabAssignments:Hide()
        RaidControlTabControls:Hide()
        RaidControlFrame:SetHeight(30)
    end

    RaidControl:RefreshAllButtons()

    DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r loaded. /rc to toggle, /rc help for commands. Triggers: |cff00ff00ON|r")
end

-- OnEvent
function RaidControl:OnEvent(frame, event, arg1)
    if event == "ADDON_LOADED" and arg1 == "RaidControl" then
        if not RaidControlDB then
            RaidControlDB = {}
        end
        for k, v in pairs(defaults) do
            if RaidControlDB[k] == nil then
                RaidControlDB[k] = v
            end
        end
        RaidControlDB.triggerCooldowns = {}
    elseif event == "PLAYER_ENTERING_WORLD" then
        -- On 1.12 ADDON_LOADED doesn't exist; saved variables are ready by PLAYER_ENTERING_WORLD
        if not RaidControlDB then
            RaidControlDB = {}
        end
        for k, v in pairs(defaults) do
            if RaidControlDB[k] == nil then
                RaidControlDB[k] = v
            end
        end
        if not RaidControlDB.triggerCooldowns then
            RaidControlDB.triggerCooldowns = {}
        end
        -- Delay dropdown init until UI is fully ready
        InitializeAllDropdowns()
    elseif event == "GROUP_ROSTER_UPDATE" or event == "PARTY_MEMBERS_CHANGED" or event == "RAID_ROSTER_UPDATE" then
        if not dropdownsInitialized then return end
        -- Tank roles
        UIDropDownMenu_Initialize(RaidControlMTDropdown, MakeTankDropdownInit(RaidControlMTDropdown, "mainTank"))
        UIDropDownMenu_Initialize(RaidControlOT1Dropdown, MakeTankDropdownInit(RaidControlOT1Dropdown, "offTank1"))
        UIDropDownMenu_Initialize(RaidControlOT2Dropdown, MakeTankDropdownInit(RaidControlOT2Dropdown, "offTank2"))
        -- Puller
        UIDropDownMenu_Initialize(RaidControlPullerDropdown, InitializePullerDropdown)
        -- Healer roles
        UIDropDownMenu_Initialize(RaidControlMTHDropdown, MakeHealerDropdownInit(RaidControlMTHDropdown, "mtHealer"))
        UIDropDownMenu_Initialize(RaidControlOT1HDropdown, MakeHealerDropdownInit(RaidControlOT1HDropdown, "ot1Healer"))
        UIDropDownMenu_Initialize(RaidControlOT2HDropdown, MakeHealerDropdownInit(RaidControlOT2HDropdown, "ot2Healer"))
    elseif event == "COMBAT_LOG_EVENT_UNFILTERED" then
        RaidControl:ProcessCombatLog()
    end
end

-- Trigger cooldown tracking
local triggerCooldowns = {}
local TRIGGER_COOLDOWN = 2.0  -- Default cooldown between same trigger firing

-- Check if trigger is on cooldown
local function IsTriggerOnCooldown(triggerKey)
    local now = GetTime()
    if triggerCooldowns[triggerKey] and (now - triggerCooldowns[triggerKey]) < TRIGGER_COOLDOWN then
        return true
    end
    return false
end

-- Set trigger cooldown
local function SetTriggerCooldown(triggerKey, cooldown)
    triggerCooldowns[triggerKey] = GetTime()
end

-- Execute trigger commands with affected player substitution
local function ExecuteTriggerCommands(trigger, affectedPlayer)
    if not trigger.commands then return end
    
    for i = 1, 20 do
        local cmd = trigger.commands[i]
        if cmd then
            local target = cmd.target or "raid"
            local command = cmd.command
            local delay = cmd.delay or 0
            
            -- Handle "affected" target - whisper the player who got the debuff
            if target == "affected" then
                if affectedPlayer then
                    target = "w:" .. affectedPlayer
                else
                    target = "raid"  -- fallback
                end
            end
            
            -- Substitute {affected} in command string
            if command and affectedPlayer then
                command = string.gsub(command, "{affected}", affectedPlayer)
                command = string.gsub(command, "{AFFECTED}", affectedPlayer)
            end
            
            if command then
                if delay > 0 then
                    local queueIdx = table.getn(commandQueue) + 1
                    commandQueue[queueIdx] = { target = target, command = command, delay = delay }
                else
                    RaidControl:ExecuteSingleCommand(command, target)
                end
            end
        else
            break
        end
    end
    
    -- Start timer if there are delayed commands
    if table.getn(commandQueue) > 0 and commandDelay == 0 then
        commandTimer = 0
        commandDelay = commandQueue[1].delay or 0
    end
end

-- ============================================================

-- Get player name from GUID
local function GetPlayerNameFromGUID(guid)
    if not guid then return nil end
    -- In Classic, we can use GetPlayerInfoByGUID
    local _, _, _, _, _, name = GetPlayerInfoByGUID(guid)
    return name
end

-- Process combat log events
function RaidControl:ProcessCombatLog()
    -- Get current boss triggers
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    if not raid then return end
    
    local boss = raid.bosses[RaidControlDB.selectedBoss]
    if not boss or not boss.triggers then return end
    
    -- Get combat log info (Classic Era API)
    local timestamp, subevent, hideCaster, sourceGUID, sourceName, sourceFlags, sourceRaidFlags,
          destGUID, destName, destFlags, destRaidFlags, spellId, spellName, spellSchool,
          auraType = CombatLogGetCurrentEventInfo()
    
    -- Debug output for all combat log events (when debug mode is on)
    if triggersDebug then
        local debugMsg = string.format("|cff888888[CL] %s: %s -> %s (spell: %s / %s)|r",
            subevent or "nil",
            sourceName or "nil", 
            destName or "nil",
            tostring(spellId),
            spellName or "nil"
        )
        DEFAULT_CHAT_FRAME:AddMessage(debugMsg)
    end
    
    -- Check each trigger
    for i = 1, 50 do
        local trigger = boss.triggers[i]
        if not trigger then break end
        
        -- Check if event matches
        if trigger.event == subevent then
            local matches = false
            local matchReason = ""
            
            -- Match by spell ID
            if trigger.spellId and trigger.spellId == spellId then
                matches = true
                matchReason = "spellId=" .. spellId
            end
            
            -- Match by spell name (case insensitive)
            if trigger.spellName and spellName and 
               string.lower(trigger.spellName) == string.lower(spellName) then
                matches = true
                matchReason = "spellName=" .. spellName
            end
            
            -- Match by source name (for specific mob abilities)
            if matches and trigger.sourceName and sourceName then
                if string.lower(trigger.sourceName) ~= string.lower(sourceName) then
                    matches = false
                    matchReason = matchReason .. " (source mismatch)"
                end
            end
            
            -- Match by destination (for player-specific effects)
            if matches and trigger.destIsPlayer and destFlags then
                local isPlayer = bit.band(destFlags, COMBATLOG_OBJECT_TYPE_PLAYER) > 0
                if not isPlayer then
                    matches = false
                    matchReason = matchReason .. " (not player)"
                end
            end
            
            -- Check if only for raid/party members
            if matches and trigger.destIsGroupMember and destFlags then
                local isGroupMember = bit.band(destFlags, COMBATLOG_OBJECT_AFFILIATION_RAID) > 0 or
                                      bit.band(destFlags, COMBATLOG_OBJECT_AFFILIATION_PARTY) > 0
                if not isGroupMember then
                    matches = false
                    matchReason = matchReason .. " (not group member)"
                end
            end
            
            -- Debug: show trigger check result
            if triggersDebug then
                local triggerName = trigger.spellName or tostring(trigger.spellId) or "unnamed"
                if matches then
                    DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00  -> Trigger MATCH: " .. triggerName .. " (" .. matchReason .. ")|r")
                end
            end
            
            if matches and trigger.commands and trigger.commands[1] then
                -- Create unique key for cooldown
                local triggerKey = (trigger.spellId or trigger.spellName or "") .. "_" .. 
                                   subevent .. "_" .. (destName or "")
                
                -- Check cooldown
                local cooldown = trigger.cooldown or TRIGGER_COOLDOWN
                if not IsTriggerOnCooldown(triggerKey) then
                    SetTriggerCooldown(triggerKey, cooldown)
                    
                    -- Get affected player name
                    local affectedPlayer = destName
                    
                    -- Debug message (optional per-trigger or global debug)
                    if trigger.debug or triggersDebug then
                        DEFAULT_CHAT_FRAME:AddMessage(
                            "|cff00ff00RaidControl|r: Trigger fired - " .. 
                            (trigger.spellName or tostring(spellId) or "unknown") .. 
                            " on " .. (affectedPlayer or "unknown")
                        )
                    end
                    
                    -- Execute commands
                    ExecuteTriggerCommands(trigger, affectedPlayer)
                elseif triggersDebug then
                    DEFAULT_CHAT_FRAME:AddMessage("|cffff8800  -> On cooldown|r")
                end
            end
        end
    end
end

-- Enable/disable triggers
local triggersEnabled = true
local triggersDebug = false  -- Debug mode for verbose output

function RaidControl:EnableTriggers()
    triggersEnabled = true
    if RC_NATIVE_CLEU then
        RaidControlFrame:RegisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
    end
    DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Triggers enabled")
end

function RaidControl:DisableTriggers()
    triggersEnabled = false
    if RC_NATIVE_CLEU then
        RaidControlFrame:UnregisterEvent("COMBAT_LOG_EVENT_UNFILTERED")
    end
    DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Triggers disabled")
end

function RaidControl:ToggleTriggers()
    if triggersEnabled then
        RaidControl:DisableTriggers()
    else
        RaidControl:EnableTriggers()
    end
end

function RaidControl:ToggleTriggersDebug()
    triggersDebug = not triggersDebug
    if triggersDebug then
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Trigger debug mode |cff00ff00ON|r - will show all combat log events")
    else
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Trigger debug mode |cffff0000OFF|r")
    end
end

-- Find and execute a button by label
local function ExecuteButtonByLabel(label)
    -- Search global categories
    for i = 1, 20 do
        local category = RaidControl_Categories[i]
        if category and category.buttons then
            for j = 1, 20 do
                local btn = category.buttons[j]
                if btn and btn.label == label then
                    ExecuteButtonCommands(btn)
                    return true
                end
            end
        elseif not category then
            break
        end
    end

    -- Search current boss categories
    local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
    if raid then
        local boss = raid.bosses[RaidControlDB.selectedBoss]
        if boss and boss.categories then
            for i = 1, 20 do
                local category = boss.categories[i]
                if category and category.buttons then
                    for j = 1, 20 do
                        local btn = category.buttons[j]
                        if btn and btn.label == label then
                            ExecuteButtonCommands(btn)
                            return true
                        end
                    end
                elseif not category then
                    break
                end
            end
        end
    end

    return false
end

-- Slash commands
SLASH_RAIDCONTROL1 = "/rc"
SLASH_RAIDCONTROL2 = "/raidcontrol"
SlashCmdList["RAIDCONTROL"] = function(msg)
    if msg == "show" then
        RaidControlFrame:Show()
    elseif msg == "hide" then
        RaidControlFrame:Hide()
    elseif msg == "reset" then
        RaidControlFrame:ClearAllPoints()
        RaidControlFrame:SetPoint("CENTER", UIParent, "CENTER", 0, 0)
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r position reset.")
    elseif msg == "setup" then
        RaidControl:RunRaidSetup()
    elseif string.sub(msg, 1, 4) == "btn " then
        local label = string.sub(msg, 5)
        if not ExecuteButtonByLabel(label) then
            DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Button '" .. label .. "' not found")
        end
    elseif msg == "triggers" or msg == "trigger" then
        RaidControl:ToggleTriggers()
    elseif msg == "triggers on" then
        RaidControl:EnableTriggers()
    elseif msg == "triggers off" then
        RaidControl:DisableTriggers()
    elseif msg == "triggers debug" or msg == "debug" then
        RaidControl:ToggleTriggersDebug()
    elseif msg == "triggers test" or msg == "test" then
        -- Test if combat log events are being received
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r: Testing trigger system...")
        local raid = RaidControl_BossData.raids[RaidControlDB.selectedRaid]
        if not raid then
            DEFAULT_CHAT_FRAME:AddMessage("  |cffff0000No raid selected!|r")
            return
        end
        DEFAULT_CHAT_FRAME:AddMessage("  Raid: " .. raid.name)
        local boss = raid.bosses[RaidControlDB.selectedBoss]
        if not boss then
            DEFAULT_CHAT_FRAME:AddMessage("  |cffff0000No boss selected!|r")
            return
        end
        DEFAULT_CHAT_FRAME:AddMessage("  Boss: " .. boss.name)
        if not boss.triggers then
            DEFAULT_CHAT_FRAME:AddMessage("  |cffff8800No triggers defined for this boss|r")
            return
        end
        local count = 0
        for i = 1, 50 do
            if boss.triggers[i] then count = count + 1 else break end
        end
        DEFAULT_CHAT_FRAME:AddMessage("  Triggers defined: " .. count)
        DEFAULT_CHAT_FRAME:AddMessage("  Use |cff00ff00/rc debug|r to see combat log events")
    elseif msg == "help" then
        DEFAULT_CHAT_FRAME:AddMessage("|cff00ff00RaidControl|r commands:")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc - toggle window")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc show|hide - show/hide window")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc setup - run raid setup")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc btn <label> - execute button by label")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc triggers - toggle combat triggers")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc debug - toggle trigger debug (shows all combat log)")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc test - test trigger configuration")
        DEFAULT_CHAT_FRAME:AddMessage("  /rc reset - reset window position")
    else
        if RaidControlFrame:IsShown() then
            RaidControlFrame:Hide()
        else
            RaidControlFrame:Show()
        end
    end
end
