(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]
            [clojure.core.async :as async :refer [close!]]
            [discljord.messaging :as discord-rest]
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]]
            [clojure.set :as set]
            [clojure.string :as string]
            [cheshire.core :as cheshire]))

(def state (atom nil))

(def bot-id (atom nil))

(def config (edn/read-string (slurp "config.edn")))
(def token (->> "secret.edn" (slurp) (edn/read-string) (:token)))

(def guild-roles (cheshire/parse-string (slurp "guild_games_roles_default.json") true))

(defmulti handle-event (fn [type _data] type))


(defmethod handle-event :ready
  [_ event-data]
  (println "logged in to guilds: " (->> event-data (:guilds) (map :id)))
  (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing config))))

(defmethod handle-event :default [_ _])

(defmethod handle-event :presence-update 
  [_ event-data]
  (println event-data)
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
                                            (#(set/intersection
                                               (string/lower-case %)
                                               activities-names))
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
        role-update (fn [f] (partial f (:rest @state) event-guild-id user-id))
        add-fut (vec (map #((role-update discord-rest/add-guild-member-role!) %) roles-to-add))
        rem-fut (vec (map #((role-update discord-rest/remove-guild-member-role!) %) roles-to-remove))]
            ;; remove-guild-member-role!  user-id role-id & {:as opts, :keys [:user-agent :audit-reason]})
            ;; (map #((println "add-fut:" (pr-str @%))) add-fut)
            ;; (map #((println "rem-fut:" (pr-str @%))) rem-fut)
            ;; (println "add-fut:" (pr-str add-fut))
            ;; (println "rem-fut:" (pr-str rem-fut))
    (println "roles to add:" (pr-str roles-to-add))
    (println "roles to remove:" (pr-str roles-to-remove))))



(defn start-bot! [token intents]
  (let [event-channel (async/chan 100)
        gateway-connection (discord-ws/connect-bot! token event-channel :intents intents)
        rest-connection (discord-rest/start-connection! token)]
    {:events  event-channel
     :gateway gateway-connection
     :rest    rest-connection}))

(defn stop-bot! [{:keys [rest gateway events] :as _state}]
  (discord-rest/stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state (start-bot! token (:intents config)))
  (reset! bot-id (:id @(discord-rest/get-current-user! (:rest @state))))
  (try
    (message-pump! (:events @state) handle-event)
    (finally (stop-bot! @state))))


;;  ( conn & {:as opts, :keys [:user-agent :audit-reason]})
;;  ( conn guild-id user-id role-id & {:as opts, :keys [:user-agent :audit-reason]})


;; (defn easter [event-data]
;;   (let [guild-ids (->> event-data (:guilds) (map :id))
;;         role-name "Lazy Null"
;;         reason "Heil the king of nothing and master of null"
;;         ]))
