#pragma once
#include "DungeonActions.h"
#include "ChangeStrategyAction.h"
#include "MovementActions.h"
#include "UseItemAction.h"
#include "playerbot/strategy/values/GuidPositionValues.h"

namespace ai
{
    const uint32 SPELL_DISARM_TRAP = 1842;

    class BlackwingLairEnableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BlackwingLairEnableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable blackwing lair strategy", "+blackwing lair") {}
    };

    class BlackwingLairDisableDungeonStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BlackwingLairDisableDungeonStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable blackwing lair strategy", "-blackwing lair") {}
    };

    class MoveToSuppressionDeviceAction : public MovementAction
    {
    public:
        MoveToSuppressionDeviceAction(PlayerbotAI* ai) : MovementAction(ai, "move to suppression device") {}

        bool Execute(Event& event) override
        {
            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos in sight,suppression devices}");
            
            if (gos.empty())
                return false;

            WorldPosition botPos(bot);
            GuidPosition closest;
            float closestDist = FLT_MAX;

            for (const GuidPosition& gp : gos)
            {
                float dist = botPos.distance(gp);
                if (dist < closestDist)
                {
                    closestDist = dist;
                    closest = gp;
                }
            }

            if (!closest)
                return false;
            
            if (ai->HasStrategy("debug move", BotState::BOT_STATE_NON_COMBAT))
            {
                ai->TellPlayerNoFacing(GetMaster(), "Moving to Suppression Device at " + std::to_string((int)closestDist) + " yards");
            }

            return MoveTo(closest.getMapId(), closest.getX(), closest.getY(), closest.getZ());
        }

        bool isPossible() override
        {
            return ai->CanMove();
        }

        bool isUseful() override
        {
            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos in sight,suppression devices}");
            return !gos.empty();
        }
    };

    class StealthForSuppressionDeviceAction : public Action
    {
    public:
        StealthForSuppressionDeviceAction(PlayerbotAI* ai) : Action(ai, "stealth for suppression device") {}

        bool Execute(Event& event) override
        {
            if (bot->getClass() != CLASS_ROGUE)
                return false;

            if (ai->HasAura("stealth", bot))
                return false;

            if (ai->CastSpell("stealth", bot))
            {
                ai->ChangeStrategy("+stealthed", BotState::BOT_STATE_COMBAT);
                ai->ChangeStrategy("+stealthed", BotState::BOT_STATE_NON_COMBAT);
                bot->InterruptSpell(CURRENT_MELEE_SPELL);
                return true;
            }

            return false;
        }

        bool isPossible() override
        {
            return bot->getClass() == CLASS_ROGUE && !ai->HasAura("stealth", bot);
        }

        bool isUseful() override
        {
            if (ai->HasAura("stealth", bot))
                return false;

            // Core rogue stealth logic had some WSG/EYE flag checks, added in here too just in case
            return !ai->HasAura(23333, bot) && !ai->HasAura(23335, bot) && !ai->HasAura(34976, bot);
        }
    };

    class DeactivateSuppressionDeviceAction : public Action
    {
    public:
        DeactivateSuppressionDeviceAction(PlayerbotAI* ai) : Action(ai, "deactivate suppression device") {}

        bool Execute(Event& event) override
        {
            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "entry filter::{gos close,suppression devices}");
            
            if (gos.empty())
                return false;

            for (const GuidPosition& guidPos : gos)
            {
                GameObject* go = ai->GetGameObject(guidPos);
                if (!go)
                    continue;

                if (go->GetLootState() != GO_READY)
                    continue;

                if (!bot->GetGameObjectIfCanInteractWith(go->GetObjectGuid(), GAMEOBJECT_TYPE_TRAP))
                    continue;

                std::unique_ptr<WorldPacket> packet(new WorldPacket(CMSG_GAMEOBJ_USE));
                *packet << go->GetObjectGuid();
                bot->GetSession()->QueuePacket(std::move(packet));

                if (ai->HasStrategy("debug move", BotState::BOT_STATE_NON_COMBAT))
                {
                    ai->TellPlayerNoFacing(GetMaster(), "Deactivating Suppression Device");
                }

                return true;
            }

            return false;
        }

        bool isPossible() override
        {
            return ai->CanMove();
        }
    };

    // Boss fight enable/disable actions
    class RazorgoreEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RazorgoreEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable razorgore fight strategy", "+razorgore") {}
    };
    class RazorgoreDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        RazorgoreDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable razorgore fight strategy", "-razorgore") {}
    };

    class VaelastraszEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        VaelastraszEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable vaelastrasz fight strategy", "+vaelastrasz") {}
    };
    class VaelastraszDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        VaelastraszDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable vaelastrasz fight strategy", "-vaelastrasz") {}
    };
    // Burning Adrenaline: run away from group before explosion
    class VaelastraszRunAwayAction : public RunAwayFromGroupAction
    {
    public:
        VaelastraszRunAwayAction(PlayerbotAI* ai) : RunAwayFromGroupAction(ai, "vaelastrasz run away") {}
    };

    class BroodlordEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BroodlordEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable broodlord fight strategy", "+broodlord") {}
    };
    class BroodlordDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        BroodlordDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable broodlord fight strategy", "-broodlord") {}
    };

    class FiremawEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FiremawEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable firemaw fight strategy", "+firemaw") {}
    };
    class FiremawDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FiremawDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable firemaw fight strategy", "-firemaw") {}
    };
    class FiremawMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        FiremawMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from firemaw", 11983, 31.0f) {}
    };

    class EbonrocEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        EbonrocEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable ebonroc fight strategy", "+ebonroc") {}
    };
    class EbonrocDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        EbonrocDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable ebonroc fight strategy", "-ebonroc") {}
    };
    class EbonrocMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        EbonrocMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from ebonroc", 14601, 31.0f) {}
    };

    class FlamegorEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FlamegorEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable flamegor fight strategy", "+flamegor") {}
    };
    class FlamegorDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        FlamegorDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable flamegor fight strategy", "-flamegor") {}
    };
    class FlamegorMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        FlamegorMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from flamegor", 11981, 31.0f) {}
    };

    class ChromaggusEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ChromaggusEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable chromaggus fight strategy", "+chromaggus") {}
    };
    class ChromaggusDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        ChromaggusDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable chromaggus fight strategy", "-chromaggus") {}
    };
    class ChromaggusMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        ChromaggusMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from chromaggus", 14020, 31.0f) {}
    };

    class NefarianEnableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NefarianEnableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "enable nefarian fight strategy", "+nefarian") {}
    };
    class NefarianDisableFightStrategyAction : public ChangeAllStrategyAction
    {
    public:
        NefarianDisableFightStrategyAction(PlayerbotAI* ai) : ChangeAllStrategyAction(ai, "disable nefarian fight strategy", "-nefarian") {}
    };
    class NefarianMoveAwayAction : public MoveAwayFromCreature
    {
    public:
        NefarianMoveAwayAction(PlayerbotAI* ai) : MoveAwayFromCreature(ai, "move away from nefarian", 11583, 31.0f) {}
    };

    class DisarmSuppressionDeviceAction : public Action
    {
    public:
        DisarmSuppressionDeviceAction(PlayerbotAI* ai) : Action(ai, "disarm suppression device") {}

        bool Execute(Event& event) override
        {
            if (bot->getClass() != CLASS_ROGUE)
                return false;

            if (!bot->HasSpell(SPELL_DISARM_TRAP))
                return false;

            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos close,suppression devices}");
            
            if (gos.empty())
                return false;

            WorldPosition botPos(bot);
            GameObject* closestGo = nullptr;
            float closestDist = FLT_MAX;

            for (const GuidPosition& guidPos : gos)
            {
                GameObject* go = ai->GetGameObject(guidPos);
                if (!go)
                    continue;

                if (go->GetLootState() != GO_READY)
                    continue;

                float dist = botPos.distance(WorldPosition(go));
                if (dist < closestDist)
                {
                    closestDist = dist;
                    closestGo = go;
                }
            }

            if (!closestGo)
                return false;

            if (ai->HasStrategy("debug move", BotState::BOT_STATE_NON_COMBAT))
            {
                ai->TellPlayerNoFacing(GetMaster(), "Casting Disarm Trap on Suppression Device");
            }

            return ai->CastSpell(SPELL_DISARM_TRAP, closestGo);
        }

        bool isPossible() override
        {
            return bot->getClass() == CLASS_ROGUE && 
                   bot->HasSpell(SPELL_DISARM_TRAP) && 
                   ai->CanMove();
        }

        bool isUseful() override
        {
            std::list<GuidPosition> gos = AI_VALUE(std::list<GuidPosition>, "go usable filter::go trapped filter::entry filter::{gos close,suppression devices}");
            return !gos.empty();
        }
    };
}