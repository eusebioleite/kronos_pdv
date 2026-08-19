create or replace trigger kronos.f_prgven_kronos_pdv_queue 
after insert or update or delete on kronos.f_prgven
for each row
declare
    v_order_code kronos.f_prgven.prv_numped%type;
begin
    if deleting then
        v_order_code := :old.prv_numped;
    else
        v_order_code := :new.prv_numped;
    end if;

    merge into kronos_pdv_queue queue
    using dual
       on (queue.order_code = v_order_code)
    when matched then
        update set 
            queue.sync       = 1,
            queue.updated_at = systimestamp
    when not matched then
        insert (order_code, sync, retries, created_at, updated_at)
        values (v_order_code, 1, 0, systimestamp, systimestamp);

exception
    when others then
        null;
end;
/