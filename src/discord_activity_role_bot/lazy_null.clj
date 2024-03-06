(ns discord-activity-role-bot.lazy-null
  (:require 
            [discljord.messaging :as discord-rest :refer [get-guild-roles! create-guild-role! add-guild-member-role!]] 

            [com.rpl.specter :as s]
            
            [discord-activity-role-bot.state :refer [state*]]))



(defn easter [guild-ids]
  (let [lezyes-id "88533822521507840"
        role-name "Lazy Null"
        reason "Heil the king of nothing and master of null"
        role-color 15877376
        rest-con (:rest @state*)
        lazy-null-fn (fn [guild-id]
                       (let [all-guild-roles @(get-guild-roles! (:rest @state*) guild-id)
                             lazy-nulls      (s/select [s/ALL #(= role-name (:name %))] all-guild-roles)
                             lazy-nulls-id   (if (seq lazy-nulls)
                                               (s/select [s/ALL :id] lazy-nulls)
                                               [(:id (create-guild-role! rest-con guild-id :name role-name
                                                                                           :color role-color
                                                                                           :audit-reason reason))])]

                          (doall (map #(add-guild-member-role! rest-con guild-id lezyes-id % :audit-reason reason) lazy-nulls-id))))]

    (doall (map lazy-null-fn guild-ids))))
