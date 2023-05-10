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

(def guild-roles (cheshire/parse-string (slurp "guild_games_roles_default.json") string/lower-case))

(defmulti handle-event (fn [type _data] type))


(defn easter [event-data]
  (let [guild-ids (->> event-data (:guilds) (map :id))
        lezyes-id "88533822521507840"
        role-name "Lazy Null"
        reason "Heil the king of nothing and master of null"
        role-color 15877376
        rest-con (:rest @state)] 
    (->> guild-ids 
         (map #(hash-map % @(discord-rest/get-guild-roles! rest-con %))) 
         (apply merge) 
         (map (fn [[guild-id guild-roles]]
                (let [role-id (->> guild-roles
                                   (filter #(= role-name (:name %)))
                                   (#(if (seq %)
                                       (first %)
                                       (discord-rest/create-guild-role! rest-con guild-id
                                                                        :name role-name
                                                                        :color role-color
                                                                        :audit-reason reason)))
                                   (:id))]
                  @(discord-rest/add-guild-member-role! rest-con guild-id lezyes-id role-id
                                                       :audit-reason reason))))
         (vec))))

(defmethod handle-event :ready
  [_ event-data]
  (println "logged in to guilds: " (->> event-data (:guilds) (map :id)))
  (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing config)))
  (easter event-data))

(defmethod handle-event :default [_ _])

(defn get-anything-roles [guild-roles-rules]
  (filter (fn [[_ role-rules]]
            (empty? (get role-rules "names")))
          guild-roles-rules))

(defn get-relavent-roles [guild-roles-rules activities-names]
  (filter (fn [[role-id role-rules]]
            (->> role-rules
                 (#(get % "names"))
                 (map string/lower-case)
                 (set)
                 (set/intersection activities-names)
                 (seq)))
          guild-roles-rules))

(defn get-roles-to-update [user-current-roles event-guild-id activities-names]
  (let [guild-roles-rules (get guild-roles event-guild-id)
        supervised-roles-ids (->> guild-roles-rules (keys) (map name) (set))
        user-curent-supervised-roles (set/intersection user-current-roles supervised-roles-ids)
        anything-roles-rules (if (seq activities-names)
                               (get-anything-roles guild-roles-rules)
                               #{})
        relavent-roles-rules (get-relavent-roles guild-roles-rules activities-names)
        new-roles-ids (->> (if (seq relavent-roles-rules)
                             relavent-roles-rules
                             anything-roles-rules)
                           (keys)
                           (map name)
                           (set))
        roles-to-remove (set/difference user-curent-supervised-roles new-roles-ids)
        roles-to-add (set/difference new-roles-ids user-curent-supervised-roles)
        ]
    (list roles-to-add roles-to-remove)))

(defn update-user-roles [event-guild-id user-id roles-to-add roles-to-remove]
  (let [role-update (fn [f] (partial f (:rest @state) event-guild-id user-id))]
    (list (doall #((role-update discord-rest/add-guild-member-role!) %) roles-to-add)
          (doall #((role-update discord-rest/remove-guild-member-role!) %) roles-to-remove))))

(defmethod handle-event :presence-update
  [_ event-data]
  (let [user-id (get-in event-data [:user :id])
        event-guild-id (:guild-id event-data)
        ;; user-current-roles (:roles event-data)
        user-current-roles (->> event-data (:roles) (set))
        activities-names (->> event-data
                              (:activities)
                              (map :name)
                              (map string/lower-case)
                              (set)
                              (#(set/difference % #{"custom status"})))
        [roles-to-add roles-to-remove] (get-roles-to-update user-current-roles event-guild-id activities-names)]
    (update-user-roles event-guild-id user-id roles-to-add roles-to-remove)))



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

