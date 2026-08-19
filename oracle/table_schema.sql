create table kronos_pdv_queue (
    order_code    varchar2(40) not null,
    sync          number(1) default 0 not null,
    retries       number(1) default 0 not null,
    error         clob,
    created_at    timestamp(6) default systimestamp not null,
    updated_at    timestamp(6) default systimestamp not null,
    constraint pk_kronos_pdv_queue primary key ( order_code ),
    constraint chk_kronos_pdv_error check ( error is json ),
    constraint chk_kronos_pdv_sync check ( sync in (0, 1) ),
    constraint chk_kronos_pdv_retries check ( retries between 0 and 5 )
);

create or replace trigger trg_kronos_pdv_queue_updated_at before
   update on kronos_pdv_queue
   for each row
begin
   :new.updated_at := systimestamp;
end;
/

comment on table kronos_pdv_queue is
   'Fila de integração do Kronos_PDV, automação responsável por transformar pedidos de venda do Debx em cards do DealerCRM.';

comment on column kronos_pdv_queue.order_code is
   'Código do pedido de venda no Debx.';

comment on column kronos_pdv_queue.sync is
   'Indica se houve alteração (1 = aguardando sync, 0 = sincronizado).';

comment on column kronos_pdv_queue.retries is
   'Contador de tentativas de sincronizar, máximo de 5.';

comment on column kronos_pdv_queue.error is
   'Histórico de erros de sincronização (JSON array de strings).';

comment on column kronos_pdv_queue.created_at is
   'Timestamp de criação.';

comment on column kronos_pdv_queue.updated_at is
   'Timestamp da última modificação.';

create index idx_kronos_pdv_queue_sync on
   kronos_pdv_queue (
      sync
   );

grant all privileges on kronos_pdv_queue to kronos;