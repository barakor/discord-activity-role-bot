(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]
            [discord-activity-role-bot.handle-presence :refer [presence-update]]
            [discord-activity-role-bot.handle-db :refer [get-db]]
            [clojure.core.async :as async :refer [close!]]
            [discljord.messaging :as discord-rest]
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]]
            [clojure.set :as set]
            [clojure.string :as string]
            [cheshire.core :as cheshire]))

(def state (atom nil))

(def db (atom nil))

(def bot-id (atom nil))

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
  (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing (:config @state))))
  (easter event-data))

(defmethod handle-event :default [_ _])

(defmethod handle-event :presence-update
  [_ event-data]
  (let [rest-connection (:rest @sate)
        db @db]) 
  (presence-update event-data rest-connection db))


(defn start-bot! [] 
  (let [token (->> "secret.edn" (slurp) (edn/read-string) (:token))
        guild-roles (cheshire/parse-string (slurp "guild_games_roles_default.json") string/lower-case)
        config (edn/read-string (slurp "config.edn"))
        intents (:intents config)
        event-channel (async/chan 100)
        gateway-connection (discord-ws/connect-bot! token event-channel :intents intents)
        rest-connection (discord-rest/start-connection! token)]
    {:events  event-channel
     :gateway gateway-connection
     :rest    rest-connection
     :config config
     :guild-roles guild-roles}))

(defn stop-bot! [{:keys [rest gateway events] :as _state}]
  (discord-rest/stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state (start-bot!))
  (reset! bot-id (:id @(discord-rest/get-current-user! (:rest @state))))
  (reset! db (get-db))
  (try
    (message-pump! (:events @state) handle-event)
    (finally (stop-bot! @state))))

