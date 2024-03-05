(ns discord-activity-role-bot.core
  (:require [clojure.edn :as edn]
            [discord-activity-role-bot.handle-presence :refer [presence-update]]
            [discord-activity-role-bot.handle-db :refer [load-db!]]
            [clojure.core.async :as async :refer [close!]]
            
            [discljord.messaging :as discord-rest :refer [get-guild-roles! create-guild-role! add-guild-member-role! 
                                                          bulk-overwrite-guild-application-commands!
                                                          start-connection! stop-connection! get-current-user!]]                                                          
            [discljord.connections :as discord-ws]
            [discljord.events :refer [message-pump!]]

            [com.rpl.specter :as s]
            
            [taoensso.timbre :as timbre :refer [log]]
            [taoensso.timbre.tools.logging :refer [use-timbre]]))
     
(use-timbre)

(def config (edn/read-string (slurp "config.edn")))

(def token (->> "secret.edn" (slurp) (edn/read-string) (:token)))

(def state (atom nil))

(def bot-id (atom nil))

(defn easter [guild-ids]
  (let [lezyes-id "88533822521507840"
        role-name "Lazy Null"
        reason "Heil the king of nothing and master of null"
        role-color 15877376
        rest-con (:rest @state)
        lazy-null-fn (fn [guild-id]
                       (let [all-guild-roles @(get-guild-roles! (:rest @state) guild-id)
                             lazy-nulls      (s/select [s/ALL #(= role-name (:name %))] all-guild-roles)
                             lazy-nulls-id   (if (seq lazy-nulls)
                                               (s/select [s/ALL :id] lazy-nulls)
                                               [(:id (create-guild-role! rest-con guild-id :name role-name
                                                                                           :color role-color
                                                                                           :audit-reason reason))])]

                          (doall (map #(add-guild-member-role! rest-con guild-id lezyes-id % :audit-reason reason) lazy-nulls-id))))]

    (doall (map lazy-null-fn guild-ids))))


(defmulti handle-event (fn [type _data] type))

(defmethod handle-event :default [event-type event-data]
  (log :report (str "event type: " event-type "\nevent-data: " event-data)))

(defmethod handle-event :ready
  [_ event-data]
  (let [guild-ids (s/select [:guilds s/ALL :id] event-data)]
    (log :info (str "logged in to guilds: " guild-ids))
    (discord-ws/status-update! (:gateway @state) :activity (discord-ws/create-activity :name (:playing config)))
    (easter guild-ids)))


(defmethod handle-event :presence-update
  [_ event-data]
  (let [rest-connection (:rest @state)] 
    (presence-update event-data rest-connection)))


(defn start-bot! [] 
  (let [intents (:intents config)
        event-channel (async/chan (async/sliding-buffer 100000))
        gateway-connection (discord-ws/connect-bot! token event-channel :intents intents)
        rest-connection (start-connection! token)]
    {:events  event-channel
     :gateway gateway-connection
     :rest    rest-connection
     :config  config}))


(defn stop-bot! [{:keys [rest gateway events] :as _state}]
  (stop-connection! rest)
  (discord-ws/disconnect-bot! gateway)
  (close! events))

(defn -main [& args]
  (reset! state (start-bot!))
  (reset! bot-id (:id @(get-current-user! (:rest @state))))
  (load-db!)
  (try
    (message-pump! (:events @state) handle-event)
    (catch Exception e (log :error (str "Exception at -main level, maybe I can handle it here? " e)))
    (finally (stop-bot! @state))))


