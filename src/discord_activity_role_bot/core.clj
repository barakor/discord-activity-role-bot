(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]
            [discord-activity-role-bot.handle-presence :refer [presence-update]]
            [discord-activity-role-bot.handle-db :refer [get-db]]
            [clojure.core.async :as async :refer [close!]]
            [discljord.messaging :as discrod-rest :refer [get-guild-roles! create-guild-role! add-guild-member-role! create-message! 
                                                                      start-connection! stop-connection! get-current-user! 
                                                                      bulk-overwrite-global-application-commands!]]
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]]))

            
(def open-command {
                   :name "test"
                   :description "Testing new command"})
  


; create-global-application-command!
(def state (atom nil))

(def db (atom nil))

(def bot-id (atom nil))


(defn easter [event-data]
  (let [guild-ids (->> event-data (:guilds) (map :id))
        lezyes-id "88533822521507840"
        role-name "Lazy Null"
        reason "Heil the king of nothing and master of null"
        role-color 15877376
        rest-con (:rest @state)] 
    (->> guild-ids 
         (map #(hash-map % @(get-guild-roles! rest-con %))) 
         (apply merge) 
         (map (fn [[guild-id guild-roles]]
                (let [role-id (->> guild-roles
                                   (filter #(= role-name (:name %)))
                                   (#(if (seq %)
                                       (first %)
                                       (create-guild-role! rest-con guild-id
                                                                        :name role-name
                                                                        :color role-color
                                                                        :audit-reason reason)))
                                   (:id))]
                  @(add-guild-member-role! rest-con guild-id lezyes-id role-id
                                                       :audit-reason reason))))
         (vec))))


(defmulti handle-event (fn [type _data] type))

(defmethod handle-event :default [_ _])


(defmethod handle-event :ready
  [_ event-data]
  (println "logged in to guilds: " (->> event-data (:guilds) (map :id)))
  (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing (:config @state))))
  (easter event-data))


(defmethod handle-event :presence-update
  [_ event-data]
  (let [rest-connection (:rest @state)
        db @db] 
      (presence-update event-data rest-connection db)))

; (defmethod handle-event :message-create
;   [event-type {{bot :bot} :author :keys [channel-id content]}]
;   (when-not bot
;     (create-message! (:rest @state) channel-id :content "Hello, World!")))

; (defmethod handle-event :message-update
;   [event-type {{bot :bot} :author :keys [channel-id content]}]
;   (when-not bot
;     (create-message! (:rest @state) channel-id :content "Hello, World!")))


; (defmethod handle-event :interaction-create
;   [_ event-data]
;   (let [{:keys [type data]} (sc/route-interaction interaction-handlers event-data)]
;     (discord-rest/create-interaction-response! (:rest @state) (:id event-data) (:token event-data) type :data data)))


(defn start-bot! [] 
  (let [token (->> "secret.edn" (slurp) (edn/read-string) (:token))
        config (edn/read-string (slurp "config.edn"))
        intents (:intents config)
        event-channel (async/chan 100)
        gateway-connection (discord-ws/connect-bot! token event-channel :intents intents)
        rest-connection (start-connection! token)]
    {:events  event-channel
     :gateway gateway-connection
     :rest    rest-connection
     :config config}))


(defn stop-bot! [{:keys [rest gateway events] :as _state}]
  (stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state (start-bot!))
  (reset! bot-id (:id @(get-current-user! (:rest @state))))
  (reset! db (get-db))
  ; (bulk-overwrite-global-application-commands! (:rest @state) @bot-id [open-command])
  (try
    (message-pump! (:events @state) handle-event)
    (finally (stop-bot! @state))))

