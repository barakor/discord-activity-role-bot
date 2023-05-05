(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]
            [clojure.core.async :as async :refer [close!]]
            [discljord.messaging :as discord-rest]
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]])
  (:require
   [clojure.set :as set]
   [clojure.string :as string]
   [cheshire.core :as cheshire]))

(def state (atom nil))

(def bot-id (atom nil))

(def config (edn/read-string (slurp "config.edn")))
(def token (->> "secret.edn" (slurp) (edn/read-string) (:token)))

(defmulti handle-event (fn [type _data] type))


(defmethod handle-event :ready
  [_ _]
  (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing config))))

(defmethod handle-event :default [_ _])

(def guild-roles (cheshire/parse-string (slurp "guild_games_roles_default.json") true))

(defn start-bot! [token & intents]
  (let [event-ch      (async/chan 100)
        connection-ch (discord-ws/connect-bot! token event-ch :intents intents)
        message-ch    (discord-rest/start-connection! token)]
    (try
      (loop []
        (let [[event-type event-data] (async/<!! event-ch)]
          (println event-type)
          (println event-data)
          (when (= event-type :presence-update)
            (let [user-id (->> event-data (:user) (:id))
                  event-guild-id (:guild-id event-data)
                  activities-names (->> event-data
                                        (:activities)
                                        (map :name)
                                        (map string/lower-case)
                                        (set)
                                        (#(set/difference % #{"custom status"})))
                  guild-roles-rules ((keyword event-guild-id) guild-roles)
                  user-current-roles (->> event-data (:roles) (set))
                  supervised-roles-ids (->> guild-roles-rules (keys) (map name) (set))
                  user-curent-supervised-roles (set/intersection user-current-roles supervised-roles-ids)
                  anything-roles-rules (if (seq activities-names)
                                         (filter (fn [[role-id role-rules]]
                                                   (empty? (:names role-rules)))
                                                 guild-roles-rules)
                                         #{})
                  relavent-roles-rules (filter (fn [[role-id role-rules]]
                                                 (->> role-rules
                                                      (:names)
                                                      (set)
                                                      (#(set/intersection % activities-names))
                                                      (seq)))
                                               guild-roles-rules)
                  new-roles-ids (->> (if (seq relavent-roles-rules)
                                       relavent-roles-rules
                                       anything-roles-rules)
                                     (keys)
                                     (map name)
                                     (set))
                  roles-to-remove (set/difference user-curent-supervised-roles new-roles-ids)
                  roles-to-add (set/difference new-roles-ids user-curent-supervised-roles)
                  role-update (fn [f] (partial f message-ch event-guild-id user-id))
                  add-fut (vec (map #((role-update discord-rest/add-guild-member-role!) %) roles-to-add))
                  rem-fut (vec (map #((role-update discord-rest/remove-guild-member-role!) %) roles-to-remove))]
            ;; remove-guild-member-role!  user-id role-id & {:as opts, :keys [:user-agent :audit-reason]})
            ;; (map #((println "add-fut:" (pr-str @%))) add-fut)
            ;; (map #((println "rem-fut:" (pr-str @%))) rem-fut)
            ;; (println "add-fut:" (pr-str add-fut))
            ;; (println "rem-fut:" (pr-str rem-fut))
              (println "roles to add:" (pr-str roles-to-add))
              (println "roles to remove:" (pr-str roles-to-remove))))
          (recur)))
      (finally
        (discord-rest/stop-connection! message-ch)
        (discord-ws/disconnect-bot!  connection-ch)
        (async/close!           event-ch)))))

(defn stop-bot! [{:keys [rest gateway events] :as _state}]
  (discord-rest/stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state (start-bot! token :guild-messages))
  (reset! bot-id (:id @(discord-rest/get-current-user! (:rest @state))))
  (try
    (message-pump! (:events @state) handle-event)
    (finally (stop-bot! @state))))

