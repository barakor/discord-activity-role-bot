(ns discord-activity-role-bot.state
   (:require
    [clojure.edn :as edn]))

(def blacklist (->> "blacklist.edn" (slurp) (edn/read-string)))
