(ns discord-activity-role-bot.state
   (:require
    [clojure.edn :as edn]))

(def blacklist (->> "blacklist.edn" (slurp) (edn/read-string)))

(defonce discord-state* (atom nil))

(defonce state* (atom nil))

(let [token (->> "secret.edn" (slurp) (edn/read-string) (:token))]
  (def config (-> "config.edn"
                (slurp)
                (edn/read-string)
                (assoc :token token))))

